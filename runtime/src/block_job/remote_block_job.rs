use std::collections::HashMap;
use std::sync::Arc;

use job::{BlockInputs, BlockJobStacks, JobId};
use mainframe::reporter::BlockReporterTx;
use manifest_meta::{HandleName, TaskBlock};
use tracing::warn;
use remote_job_client::{CreateTaskRequest, RemoteJobClient, TaskStatus};
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
const LOGS_PAGE_SIZE: usize = 100;
const LOGS_DRAIN_INTERVAL_MS: u64 = 1000;
const LOGS_DRAIN_MAX_EMPTY_ROUNDS: u32 = 10;

fn is_valid_reporter_message(val: &serde_json::Value) -> bool {
    val.get("type").and_then(|t| t.as_str()).is_some()
}

/// Immutable context shared across poll_logs / drain_logs calls.
struct LogPollCtx<'a> {
    client: &'a RemoteJobClient,
    task_id: &'a str,
    reporter: &'a BlockReporterTx,
    block_status: &'a BlockStatusTx,
    job_id: &'a JobId,
}

/// Poll one round of remote logs. Returns `(new_items, saw_session_finished)`.
/// `saw_session_finished` is true when a root-level `SessionFinished` event
/// (empty stacks) was encountered — this is the definitive last event produced
/// by the remote server session.
async fn poll_logs(
    ctx: &LogPollCtx<'_>,
    logs_page: &mut u32,
    logs_sent_on_page: &mut usize,
    finished: &mut bool,
) -> (usize, bool) {
    let mut new_items = 0;
    let mut saw_session_finished = false;
    loop {
        match ctx.client.get_task_logs(ctx.task_id, *logs_page).await {
            Ok(items) => {
                let page_len = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    let has_remote_stacks = item
                        .get("stacks")
                        .and_then(|s| s.as_array())
                        .is_some_and(|arr| !arr.is_empty());
                    let msg_type = item.get("type").and_then(|t| t.as_str());

                    // Detect root-level SessionFinished as the definitive
                    // "all logs produced" signal, even for entries already sent.
                    if !saw_session_finished
                        && !has_remote_stacks
                        && msg_type == Some("SessionFinished")
                    {
                        saw_session_finished = true;
                    }

                    if i < *logs_sent_on_page {
                        continue;
                    }

                    // Intercept root-level events for local scheduling.
                    // Defer block_status.finish() until after forward_remote_log
                    // so the reporter event is enqueued before the scheduling
                    // side triggers session teardown (exit(0)).
                    let mut should_finish = false;
                    let mut finish_error: Option<String> = None;
                    let mut finish_result: Option<HashMap<HandleName, Arc<OutputValue>>> = None;
                    if !has_remote_stacks {
                        match msg_type {
                            Some("BlockOutput") => {
                                if let (Some(output), Some(handle)) = (
                                    item.get("output").cloned(),
                                    item.get("handle").and_then(|h| h.as_str()),
                                ) {
                                    ctx.block_status.output(
                                        ctx.job_id.clone(),
                                        Arc::new(OutputValue::new(output, true)),
                                        HandleName::new(handle.to_owned()),
                                        None,
                                    );
                                }
                            }
                            Some("BlockOutputs") => {
                                if let Some(obj) = item.get("outputs").and_then(|o| o.as_object())
                                {
                                    let outputs = obj
                                        .iter()
                                        .map(|(k, v)| {
                                            (
                                                HandleName::new(k.clone()),
                                                Arc::new(OutputValue::new(v.clone(), true)),
                                            )
                                        })
                                        .collect();
                                    ctx.block_status.outputs(ctx.job_id.clone(), outputs);
                                }
                            }
                            Some("BlockFinished") | Some("SubflowBlockFinished") => {
                                if !*finished {
                                    finish_error = item
                                        .get("error")
                                        .and_then(|e| e.as_str())
                                        .map(|s| s.to_owned());
                                    finish_result = item
                                        .get("result")
                                        .and_then(|r| r.as_object())
                                        .map(|obj| {
                                            obj.iter()
                                                .map(|(k, v)| {
                                                    (
                                                        HandleName::new(k.clone()),
                                                        Arc::new(OutputValue::new(v.clone(), true)),
                                                    )
                                                })
                                                .collect()
                                        });
                                    should_finish = true;
                                }
                            }
                            _ => {}
                        }
                    }

                    if is_valid_reporter_message(&item) {
                        ctx.reporter.forward_remote_log(item);
                    } else {
                        warn!(
                            "Ignoring unknown remote log entry: {}",
                            item.get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("<missing type>")
                        );
                    }

                    if should_finish {
                        ctx.block_status.finish(ctx.job_id.clone(), finish_result, finish_error, None);
                        *finished = true;
                    }
                    *logs_sent_on_page += 1;
                    new_items += 1;
                }
                if page_len >= LOGS_PAGE_SIZE && *logs_sent_on_page >= LOGS_PAGE_SIZE {
                    *logs_page += 1;
                    *logs_sent_on_page = 0;
                    continue;
                }
                break;
            }
            Err(e) => {
                warn!(
                    "Failed to poll remote logs for task {} page {}: {}",
                    ctx.task_id, *logs_page, e
                );
                break;
            }
        }
    }
    (new_items, saw_session_finished)
}

/// Drain remaining remote logs after terminal status. The logs API may lag
/// behind the status change, so keep polling until either:
/// - a root-level `SessionFinished` is seen (definitive end signal), or
/// - no new items arrive for `LOGS_DRAIN_MAX_EMPTY_ROUNDS` consecutive rounds.
async fn drain_logs(
    ctx: &LogPollCtx<'_>,
    logs_page: &mut u32,
    logs_sent_on_page: &mut usize,
    finished: &mut bool,
) {
    let interval = std::time::Duration::from_millis(LOGS_DRAIN_INTERVAL_MS);
    let mut empty_rounds: u32 = 0;
    loop {
        let (count, saw_session_finished) =
            poll_logs(ctx, logs_page, logs_sent_on_page, finished).await;
        if saw_session_finished {
            break;
        }
        if count == 0 {
            empty_rounds += 1;
            if empty_rounds >= LOGS_DRAIN_MAX_EMPTY_ROUNDS {
                break;
            }
        } else {
            empty_rounds = 0;
        }
        tokio::time::sleep(interval).await;
    }
}

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
                     Set --remote-block-url or OOCANA_REMOTE_BLOCK_URL."
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

    let mut client = RemoteJobClient::new(&config.base_url);
    if let Some(ref token) = config.auth_token {
        client = client.with_token(token);
    }

    let payload = CreateTaskRequest::new(
        package_name.clone(),
        package_version.clone(),
        block_name.clone(),
        input_values,
    );

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
        let task_id = match client.create_remote_job(&payload).await {
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

        // Logs polling state
        let mut logs_page: u32 = 1;
        let mut logs_sent_on_page: usize = 0;
        let mut finished = false;

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

            // Poll remote logs — outputs and finish are dispatched from here.
            let log_ctx = LogPollCtx {
                client: &client,
                task_id: &task_id,
                reporter: &reporter_clone,
                block_status: &block_status_clone,
                job_id: &job_id_clone,
            };
            let _ = poll_logs(
                &log_ctx,
                &mut logs_page,
                &mut logs_sent_on_page,
                &mut finished,
            )
            .await;

            match detail.status {
                TaskStatus::Success | TaskStatus::Failed => {
                    // Drain remaining logs — BlockFinished in the log stream
                    // drives reporter.finished() and block_status.finish().
                    drain_logs(
                        &log_ctx,
                        &mut logs_page,
                        &mut logs_sent_on_page,
                        &mut finished,
                    )
                    .await;

                    // Fallback: if logs never contained a root-level
                    // BlockFinished, finish from the status detail.
                    if !finished {
                        let error = if detail.status == TaskStatus::Failed {
                            Some(
                                detail
                                    .failed_message
                                    .unwrap_or_else(|| format!("Remote task {task_id} failed")),
                            )
                        } else {
                            None
                        };
                        reporter_clone.finished(None, error.clone());
                        block_status_clone.finish(job_id_clone, None, error, None);
                    }
                    return;
                }
                _ => {} // Queued, Scheduling, Scheduled, Running — keep polling
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
