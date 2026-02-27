use std::collections::HashMap;
use std::sync::Arc;

use job::{BlockInputs, BlockJobStacks, JobId};
use manifest_meta::{HandleName, TaskBlock};
use tracing::warn;
use user_task_client::{CreateUserTaskRequest, TaskResult, TaskStatus, UserTaskClient};
use utils::output::OutputValue;

use crate::block_status::BlockStatusTx;
use crate::shared::Shared;

use super::job_handle::BlockJobHandle;

pub struct RemoteBlockJobParameters {
    pub task_block: Arc<TaskBlock>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
}

struct RemoteBlockJobHandle {
    spawn_handle: tokio::task::JoinHandle<()>,
}

impl Drop for RemoteBlockJobHandle {
    fn drop(&mut self) {
        self.spawn_handle.abort();
    }
}

const MAX_CONSECUTIVE_POLL_ERRORS: u32 = 3;
const POLL_INTERVAL_SECS: u64 = 2;
const DEFAULT_TIMEOUT_SECS: u64 = 1800; // 30 minutes

pub fn execute_remote_block_job(params: RemoteBlockJobParameters) -> Option<BlockJobHandle> {
    let RemoteBlockJobParameters {
        task_block,
        shared,
        stacks,
        job_id,
        inputs,
        block_status,
    } = params;

    let config = match shared.remote_task_config.as_ref() {
        Some(c) => c.clone(),
        None => {
            warn!("Remote task execution requested but no API configuration provided");
            block_status.finish(
                job_id,
                None,
                Some(
                    "Remote task execution requested but no API configuration provided. \
                     Set --task-api-url or OOCANA_TASK_API_URL."
                        .to_owned(),
                ),
                None,
            );
            return None;
        }
    };

    let reporter = Arc::new(shared.reporter.block(
        job_id.clone(),
        task_block.path_str(),
        stacks.clone(),
    ));

    reporter.started(&inputs);

    let (package_name, package_version, block_name) = match infer_remote_params(&task_block) {
        Ok(p) => p,
        Err(err) => {
            reporter.finished(None, Some(err.clone()));
            block_status.finish(job_id, None, Some(err), None);
            return None;
        }
    };

    let input_values = inputs.as_ref().map(|inputs| {
        let mut map = serde_json::Map::new();
        for (handle, value) in inputs.iter() {
            map.insert(handle.to_string(), value.value.clone());
        }
        map
    });

    let mut client = UserTaskClient::new(&config.base_url);
    if let Some(ref token) = config.auth_token {
        client = client.with_bearer_token(token);
    }

    let payload = CreateUserTaskRequest::Serverless {
        package_name: package_name.clone(),
        package_version: package_version.clone(),
        block_name: block_name.clone(),
        input_values,
    };

    // Timeout priority: per-block metadata > CLI/env var > default 30min
    let timeout_secs = task_block
        .remote_timeout
        .or(config.timeout_secs)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    let reporter_clone = Arc::clone(&reporter);
    let block_status_clone = block_status;
    let job_id_clone = job_id;

    let spawn_handle = tokio::spawn(async move {
        // 1. Create remote task
        let task_id = match client.create_user_task(&payload).await {
            Ok(id) => id,
            Err(e) => {
                let msg = format!("Failed to create remote task: {e}");
                reporter_clone.finished(None, Some(msg.clone()));
                block_status_clone.finish(job_id_clone, None, Some(msg), None);
                return;
            }
        };

        reporter_clone.log(
            &format!("Remote task created: {task_id}"),
            "remote_task",
        );

        // 2. Poll until terminal state or timeout (0 = no timeout)
        let poll_interval = std::time::Duration::from_secs(POLL_INTERVAL_SECS);
        let deadline = if timeout_secs > 0 {
            Some(tokio::time::Instant::now() + std::time::Duration::from_secs(timeout_secs))
        } else {
            None
        };
        let mut consecutive_errors: u32 = 0;

        loop {
            tokio::time::sleep(poll_interval).await;

            if let Some(dl) = deadline {
                if tokio::time::Instant::now() >= dl {
                    let msg = format!(
                        "Remote task {task_id} timed out after {timeout_secs}s"
                    );
                    reporter_clone.finished(None, Some(msg.clone()));
                    block_status_clone.finish(job_id_clone, None, Some(msg), None);
                    return;
                }
            }

            let detail = match client.get_task_detail(&task_id).await {
                Ok(d) => {
                    consecutive_errors = 0;
                    d
                }
                Err(e) => {
                    consecutive_errors += 1;
                    reporter_clone.log(
                        &format!(
                            "Poll error ({consecutive_errors}/{MAX_CONSECUTIVE_POLL_ERRORS}): {e}"
                        ),
                        "remote_task",
                    );
                    if consecutive_errors >= MAX_CONSECUTIVE_POLL_ERRORS {
                        let msg = format!(
                            "Remote task {task_id} polling failed after \
                             {MAX_CONSECUTIVE_POLL_ERRORS} consecutive errors: {e}"
                        );
                        reporter_clone.finished(None, Some(msg.clone()));
                        block_status_clone.finish(job_id_clone, None, Some(msg), None);
                        return;
                    }
                    continue;
                }
            };

            block_status_clone.progress(job_id_clone.clone(), detail.progress as f32);

            match detail.status {
                TaskStatus::Success => {
                    // 3. Fetch result
                    match client.get_task_result(&task_id).await {
                        Ok(TaskResult::Success { result_data, .. }) => {
                            let first_map = result_data.and_then(|mut v| {
                                if v.is_empty() {
                                    None
                                } else {
                                    Some(v.remove(0))
                                }
                            });

                            if let Some(map) = first_map {
                                let mut reporter_map = HashMap::new();
                                let mut output_map = HashMap::new();
                                for (key, value) in map {
                                    reporter_map.insert(key.clone(), value.clone());
                                    output_map.insert(
                                        HandleName::new(key),
                                        Arc::new(OutputValue::new(value, true)),
                                    );
                                }
                                reporter_clone.finished(Some(reporter_map), None);
                                block_status_clone.finish(
                                    job_id_clone,
                                    Some(output_map),
                                    None,
                                    None,
                                );
                            } else {
                                reporter_clone.finished(None, None);
                                block_status_clone.finish(job_id_clone, None, None, None);
                            }
                        }
                        Ok(_) => {
                            let msg = format!(
                                "Remote task {task_id} in unexpected result state after success"
                            );
                            reporter_clone.finished(None, Some(msg.clone()));
                            block_status_clone.finish(job_id_clone, None, Some(msg), None);
                        }
                        Err(e) => {
                            let msg = format!("Failed to get remote task result: {e}");
                            reporter_clone.finished(None, Some(msg.clone()));
                            block_status_clone.finish(job_id_clone, None, Some(msg), None);
                        }
                    }
                    return;
                }
                TaskStatus::Failed => {
                    let msg = detail
                        .failed_message
                        .unwrap_or_else(|| format!("Remote task {task_id} failed"));
                    reporter_clone.finished(None, Some(msg.clone()));
                    block_status_clone.finish(job_id_clone, None, Some(msg), None);
                    return;
                }
                _ => {} // Queued, Scheduling, Scheduled, Running - keep polling
            }
        }
    });

    Some(BlockJobHandle::new(RemoteBlockJobHandle { spawn_handle }))
}

/// Infer package_name, package_version, block_name from the TaskBlock's path metadata.
fn infer_remote_params(
    task_block: &TaskBlock,
) -> std::result::Result<(String, String, String), String> {
    let block_path = task_block
        .path
        .as_ref()
        .ok_or("Cannot infer remote params: task block has no path (inline block)")?;

    // block_name: parent dir name, e.g. tasks/my_block/block.oo.yaml -> my_block
    let block_name = block_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_owned())
        .ok_or("Cannot infer block_name from block path")?;

    let pkg_path = task_block
        .package_path
        .as_ref()
        .ok_or("Cannot infer package info: task block has no package_path")?;

    let pkg_file = manifest_reader::path_finder::find_package_file(pkg_path)
        .ok_or("Cannot find package.oo.yaml in package directory")?;

    let pkg_meta = manifest_reader::reader::read_package(&pkg_file)
        .map_err(|e| format!("Failed to read package metadata: {e}"))?;

    let package_name = pkg_meta
        .name
        .ok_or("Package has no name field in package.oo.yaml")?;
    let package_version = pkg_meta
        .version
        .ok_or("Package has no version field in package.oo.yaml")?;

    Ok((package_name, package_version, block_name))
}
