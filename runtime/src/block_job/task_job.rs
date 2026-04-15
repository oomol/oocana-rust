use mainframe::reporter::BlockReporterTx;
use mainframe::scheduler::{self, ExecutorParams, SchedulerTx};
use manifest_meta::{
    HandleName, InputDefPatchMap, InputHandles, OutputHandles, SubflowBlock, TaskBlock,
    TaskBlockExecutor,
};
use manifest_reader::manifest::SpawnOptions;
use reqwest::{Client, Url};

use std::collections::HashMap;
use std::path::Path;
use std::process;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::block_status::BlockStatusTx;
use crate::shared::Shared;

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope, SessionId};
use utils::error::Result;
use utils::path::to_absolute;

use super::job_handle::BlockJobHandle;
use super::listener::{ListenerParameters, listen_to_worker};

const CONNECTOR_BASE_URL_ENV_KEY: &str = "CONNECTOR_BASE_URL";
const DEFAULT_CONNECTOR_REQUEST_TIMEOUT_SECS: u64 = 30;
const CONNECTOR_ERROR_BODY_LIMIT: usize = 512;

fn connector_http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(Client::new)
}

pub struct TaskJobHandle {
    pub job_id: JobId,
    shared: Arc<Shared>,
    child: Option<process::Child>,
    spawn_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for TaskJobHandle {
    fn drop(&mut self) {
        self.shared
            .scheduler_tx
            .unregister_subscriber(self.job_id.to_owned());
        self.shared
            .delay_abort_tx
            .send(self.spawn_handles.drain(..).collect());
        if let Some(mut child) = self.child.take() {
            _ = child.kill();
            drop(child);
        }
    }
}

pub struct TaskJobParameters {
    pub executor: Arc<TaskBlockExecutor>,
    pub block_path: Option<String>,
    pub inputs_def: Option<InputHandles>, // block's inputs def missing some node added inputs,
    pub outputs_def: Option<OutputHandles>, // block's outputs def will miss additional outputs added on node
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    pub scope: RuntimeScope,
    pub timeout: Option<u64>,
    pub injection_store: Option<manifest_meta::InjectionStore>,
    pub flow_path: Option<String>,
    pub dir: String,
    pub inputs_def_patch: Option<InputDefPatchMap>,
}

pub fn execute_task_job(params: TaskJobParameters) -> Option<BlockJobHandle> {
    let TaskJobParameters {
        executor,
        block_path,
        inputs_def,
        outputs_def,
        shared,
        stacks,
        job_id,
        inputs,
        dir: block_dir,
        injection_store,
        flow_path,
        block_status,
        scope,
        timeout,
        inputs_def_patch,
    } = params;
    let reporter = Arc::new(shared.reporter.block(
        job_id.to_owned(),
        block_path.clone(),
        stacks.clone(),
    ));

    reporter.started(&inputs);

    let mut spawn_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    if let Some(timeout_value) = timeout {
        let timeout_handle = timeout_abort(
            job_id.to_owned(),
            std::time::Duration::from_secs(timeout_value),
            block_status.clone(),
            Arc::clone(&reporter),
        );

        spawn_handles.push(timeout_handle);
    }

    let worker_listener_handle = listen_to_worker(ListenerParameters {
        job_id: job_id.to_owned(),
        block_path: block_path.clone(),
        stacks: stacks.clone(),
        scheduler_tx: shared.scheduler_tx.clone(),
        inputs: inputs.clone(),
        outputs_def: outputs_def.clone(),
        inputs_def,
        block_status: block_status.clone(),
        reporter: Arc::clone(&reporter),
        executor: Some(executor.clone()),
        service: None,
        block_dir: block_dir.clone(),
        scope: scope.clone(),
        injection_store: injection_store.clone(),
        flow_path: flow_path.clone(),
        inputs_def_patch,
    });

    match executor.as_ref() {
        TaskBlockExecutor::Rust(e) => {
            let execute_result = spawn(
                &e.options,
                &block_dir,
                &shared.address,
                &shared.session_id,
                &job_id,
            );

            match execute_result {
                Ok(mut child) => {
                    spawn_handles.push(worker_listener_handle);
                    bind_stdio(&mut child, &reporter, &mut spawn_handles);

                    Some(BlockJobHandle::new(TaskJobHandle {
                        job_id,
                        shared,
                        child: Some(child),
                        spawn_handles,
                    }))
                }
                Err(e) => {
                    worker_listener_handle.abort();
                    reporter.finished(None, Some(e.to_string()));
                    Some(BlockJobHandle::new(TaskJobHandle {
                        job_id,
                        shared,
                        child: None,
                        spawn_handles,
                    }))
                }
            }
        }
        TaskBlockExecutor::Shell(_) => {
            let execute_result = spawn_shell(&block_dir, inputs, &shared.session_id, &job_id);

            match execute_result {
                Ok(mut child) => {
                    spawn_handles.push(worker_listener_handle);
                    let stdio_handles = bind_shell_stdio(
                        &mut child,
                        &reporter,
                        shared.scheduler_tx.clone(),
                        &shared.session_id,
                        job_id.clone(),
                    );

                    let job_id_clone = job_id.clone();
                    let shared_clone = Arc::clone(&shared);

                    let exit_handler = tokio::spawn(async move {
                        // Wait for stdio to be fully read before checking exit status
                        for handle in stdio_handles {
                            let _ = handle.await;
                        }

                        let status = child.wait().await.unwrap();
                        let status_code = status.code().unwrap_or(-1);
                        if status_code != 0 {
                            let msg = format!(
                                "Task block '{block_path:?}' exited with code {status_code}"
                            );
                            shared_clone.scheduler_tx.send_block_event(
                                scheduler::ReceiveMessage::BlockFinished {
                                    session_id: shared_clone.session_id.clone(),
                                    job_id: job_id_clone.clone(),
                                    result: None,
                                    error: Some(msg),
                                },
                            );
                        } else {
                            shared_clone.scheduler_tx.send_block_event(
                                scheduler::ReceiveMessage::BlockFinished {
                                    session_id: shared_clone.session_id.clone(),
                                    job_id: job_id_clone.clone(),
                                    result: None,
                                    error: None,
                                },
                            );
                        }
                    });

                    spawn_handles.push(exit_handler);

                    Some(BlockJobHandle::new(TaskJobHandle {
                        job_id,
                        shared,
                        child: None,
                        spawn_handles,
                    }))
                }
                Err(_) => {
                    shared.scheduler_tx.send_block_event(
                        scheduler::ReceiveMessage::BlockFinished {
                            session_id: shared.session_id.clone(),
                            job_id: job_id.clone(),
                            result: None,
                            error: Some("Failed to spawn shell".to_owned()),
                        },
                    );
                    Some(BlockJobHandle::new(TaskJobHandle {
                        job_id,
                        shared,
                        child: None,
                        spawn_handles,
                    }))
                }
            }
        }
        TaskBlockExecutor::Connector(e) => {
            let action_id = e.options.action.clone();
            let scheduler_tx = shared.scheduler_tx.clone();
            let session_id = shared.session_id.clone();
            let job_id_clone = job_id.clone();
            let connector_inputs = inputs.clone();
            let connector_outputs_def = outputs_def.clone();
            let connector_auth_token = shared.connector_auth_token.clone();
            let request_timeout =
                Duration::from_secs(timeout.unwrap_or(DEFAULT_CONNECTOR_REQUEST_TIMEOUT_SECS));

            let connector_handle = tokio::spawn(async move {
                let result = if let Some(ref token) = connector_auth_token {
                    run_connector_action_with_auth_token(
                        &action_id,
                        connector_inputs,
                        connector_outputs_def,
                        request_timeout,
                        Some(token),
                    )
                    .await
                } else {
                    run_connector_action(
                        &action_id,
                        connector_inputs,
                        connector_outputs_def,
                        request_timeout,
                    )
                    .await
                };

                match result {
                    Ok(outputs) => {
                        scheduler_tx.send_block_event(scheduler::ReceiveMessage::BlockFinished {
                            session_id,
                            job_id: job_id_clone,
                            result: Some(outputs),
                            error: None,
                        });
                    }
                    Err(error) => {
                        scheduler_tx.send_block_event(scheduler::ReceiveMessage::BlockFinished {
                            session_id,
                            job_id: job_id_clone,
                            result: None,
                            error: Some(error.to_string()),
                        });
                    }
                }
            });

            spawn_handles.push(worker_listener_handle);
            spawn_handles.push(connector_handle);

            Some(BlockJobHandle::new(TaskJobHandle {
                job_id,
                shared,
                child: None,
                spawn_handles,
            }))
        }
        _ => {
            shared.scheduler_tx.send_to_executor(ExecutorParams {
                executor_name: executor.name(),
                job_id: job_id.to_owned(),
                stacks: stacks.vec(),
                dir: block_dir.clone(),
                executor: &executor,
                outputs: &outputs_def,
                scope: &scope,
                injection_store: &injection_store,
                flow_path: &flow_path,
            });

            spawn_handles.push(worker_listener_handle);

            Some(BlockJobHandle::new(TaskJobHandle {
                job_id,
                shared,
                child: None,
                spawn_handles,
            }))
        }
    }
}

pub fn block_dir(
    task_block: &TaskBlock,
    parent_flow: Option<&Arc<RwLock<SubflowBlock>>>,
    scope: Option<&RuntimeScope>,
) -> String {
    // Priority is given to the `scope` parameter if provided, as it represents
    // the package path associated with the running package scope. If `scope` is
    // not available, the function falls back to the `block_dir` method of the
    // `task_block`. If neither is available, it uses the parent directory of the
    // `parent_flow` path, defaulting to the current directory ("./") if all else fails.
    if let Some(s) = scope.filter(|s| s.is_inject()) {
        s.path().to_string_lossy().to_string()
    } else if let Some(block_dir) = task_block.block_dir() {
        block_dir.to_string_lossy().to_string()
    } else {
        parent_flow
            .and_then(|f| {
                let flow_guard = f.read().unwrap();
                flow_guard
                    .path
                    .parent()
                    .map(|p| p.to_str().unwrap_or(".").to_owned())
            })
            .unwrap_or(".".to_owned())
    }
}

fn spawn_shell(
    dir: &str,
    inputs: Option<BlockInputs>,
    session_id: &SessionId,
    job_id: &JobId,
) -> Result<tokio::process::Child> {
    let mut envs = HashMap::new();
    envs.insert("OOCANA_SESSION_ID".to_string(), session_id.to_string());
    envs.insert("OOCANA_JOB_ID".to_string(), job_id.to_string());

    let arg = get_string_value_from_inputs(&inputs, "command");

    // 用户设置 cwd 在这里的意义不大，造成的问题反而可能更多，考虑去掉。
    let cwd = match get_string_value_from_inputs(&inputs, "cwd") {
        Some(cwd) => {
            let path = std::path::Path::new(&cwd);
            if path.is_relative() {
                std::path::Path::new(&dir)
                    .join(path)
                    .to_str()
                    .unwrap()
                    .to_owned()
            } else {
                cwd
            }
        }
        None => dir.to_owned(),
    };

    let env_strings = get_string_value_from_inputs(&inputs, "envs");
    if let Some(env_str) = env_strings {
        let env_key_value_pairs: Vec<&str> = env_str.split(',').collect();
        for env in env_key_value_pairs {
            let key_value: Vec<&str> = env.splitn(2, '=').collect();
            if key_value.len() == 2 {
                envs.insert(key_value[0].to_string(), key_value[1].to_string());
            }
        }
    }

    let mut command = tokio::process::Command::new("sh");

    // 如果是相对链接，根据 block_dir 来处理相对地址。rust 如果使用 canonicalize 如果文件不存在会直接报错
    let canonicalize = Path::new(&cwd).canonicalize();
    match canonicalize {
        Ok(cwd) => {
            command.current_dir(cwd);
        }
        Err(err) => {
            return Err(utils::error::Error::new(&format!(
                "Failed to canonicalize cwd {cwd} with error: {err}"
            )));
        }
    }

    let reporter_err = arg.clone();

    command
        .arg("-c")
        .args(arg)
        .envs(envs)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()
        .map_err(move |e| {
            utils::error::Error::with_source(
                &format!(
                    "Failed to execute 'sh {} <...OOCANA_ARGS>' at '{}'",
                    reporter_err.unwrap_or_default(),
                    cwd,
                ),
                Box::new(e),
            )
        })
}

fn get_string_value_from_inputs(inputs: &Option<BlockInputs>, key: &str) -> Option<String> {
    inputs
        .as_ref()
        .and_then(|inputs| inputs.get(&HandleName::new(key.to_string())))
        .and_then(|v| v.value.as_str().map(|s| s.to_owned()))
}

async fn run_connector_action(
    action_id: &str,
    inputs: Option<BlockInputs>,
    outputs_def: Option<OutputHandles>,
    request_timeout: Duration,
) -> Result<HashMap<HandleName, serde_json::Value>> {
    let auth_token = std::env::var("OOMOL_TOKEN")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    run_connector_action_with_auth_token(
        action_id,
        inputs,
        outputs_def,
        request_timeout,
        auth_token.as_deref(),
    )
    .await
}

async fn run_connector_action_with_auth_token(
    action_id: &str,
    inputs: Option<BlockInputs>,
    outputs_def: Option<OutputHandles>,
    request_timeout: Duration,
    auth_token: Option<&str>,
) -> Result<HashMap<HandleName, serde_json::Value>> {
    let auth_token = auth_token
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
        .or_else(|| {
            std::env::var("OOMOL_TOKEN")
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        });
    let base_url = std::env::var(CONNECTOR_BASE_URL_ENV_KEY).map_err(|_| {
        utils::error::Error::new(&format!(
            "{CONNECTOR_BASE_URL_ENV_KEY} is required for connector executor"
        ))
    })?;

    run_connector_action_with_base_url_and_auth(
        &base_url,
        action_id,
        inputs,
        outputs_def,
        request_timeout,
        auth_token.as_deref(),
    )
    .await
}

#[cfg(test)]
async fn run_connector_action_with_base_url(
    base_url: &str,
    action_id: &str,
    inputs: Option<BlockInputs>,
    outputs_def: Option<OutputHandles>,
    request_timeout: Duration,
) -> Result<HashMap<HandleName, serde_json::Value>> {
    let auth_token = std::env::var("OOMOL_TOKEN")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    run_connector_action_with_base_url_and_auth(
        base_url,
        action_id,
        inputs,
        outputs_def,
        request_timeout,
        auth_token.as_deref(),
    )
    .await
}

async fn run_connector_action_with_base_url_and_auth(
    base_url: &str,
    action_id: &str,
    inputs: Option<BlockInputs>,
    outputs_def: Option<OutputHandles>,
    request_timeout: Duration,
    auth_token: Option<&str>,
) -> Result<HashMap<HandleName, serde_json::Value>> {
    let url = build_connector_action_url(base_url, action_id)?;

    let request_body = serde_json::json!({
        "input": serialize_connector_inputs(inputs),
    });

    let request = connector_http_client()
        .post(url.clone())
        .json(&request_body)
        .timeout(request_timeout);
    let request = if let Some(token) = auth_token {
        request.bearer_auth(token)
    } else {
        request
    };

    let response = request
        .send()
        .await
        .map_err(|e| {
            utils::error::Error::with_source(
                &format!("Failed to call connector action '{action_id}' at '{url}'"),
                Box::new(e),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = format_connector_http_error(action_id, status, &body);

        return Err(utils::error::Error::new(&message));
    }

    let response_json = response.json::<serde_json::Value>().await.map_err(|e| {
        utils::error::Error::with_source(
            &format!("Failed to parse connector action '{action_id}' response as JSON"),
            Box::new(e),
        )
    })?;

    parse_connector_outputs(action_id, response_json, outputs_def.as_ref())
}

fn build_connector_action_url(base_url: &str, action_id: &str) -> Result<Url> {
    let mut url = Url::parse(base_url).map_err(|e| {
        utils::error::Error::with_source(
            &format!("Invalid {CONNECTOR_BASE_URL_ENV_KEY}: '{base_url}'"),
            Box::new(e),
        )
    })?;

    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            utils::error::Error::new(&format!(
                "Invalid {CONNECTOR_BASE_URL_ENV_KEY}: '{base_url}'"
            ))
        })?;
        segments.pop_if_empty();
        segments.extend(["v1", "actions", action_id]);
    }

    Ok(url)
}

fn serialize_connector_inputs(inputs: Option<BlockInputs>) -> HashMap<String, serde_json::Value> {
    inputs
        .unwrap_or_default()
        .into_iter()
        .map(|(handle, value)| (handle.to_string(), value.value.clone()))
        .collect()
}

fn truncate_connector_error_body(body: &str) -> String {
    let mut chars = body.chars();
    let truncated: String = chars.by_ref().take(CONNECTOR_ERROR_BODY_LIMIT).collect();
    if chars.next().is_some() {
        format!("{truncated}...<truncated>")
    } else {
        truncated
    }
}

fn format_connector_http_error(
    action_id: &str,
    status: reqwest::StatusCode,
    body: &str,
) -> String {
    if body.is_empty() {
        return format!("Connector action '{action_id}' failed with status {status}");
    }

    if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(details) = format_connector_error_details(&response_json) {
            return format!("Connector action '{action_id}' failed with status {status}: {details}");
        }
    }

    let truncated = truncate_connector_error_body(body);
    format!("Connector action '{action_id}' failed with status {status}: {truncated}")
}

fn format_connector_error_details(response_json: &serde_json::Value) -> Option<String> {
    let message = response_json
        .get("message")
        .and_then(|value| value.as_str())
        .filter(|message| !message.trim().is_empty())
        .or_else(|| {
            response_json
                .get("errorMessage")
                .and_then(|value| value.as_str())
                .filter(|message| !message.trim().is_empty())
        })?;

    let mut metadata = Vec::new();

    if let Some(error_code) = response_json
        .get("errorCode")
        .and_then(|value| value.as_str())
        .filter(|error_code| !error_code.trim().is_empty())
    {
        metadata.push(format!("errorCode={error_code}"));
    }

    if let Some(action_id) = response_json
        .get("meta")
        .and_then(|value| value.get("actionId"))
        .and_then(|value| value.as_str())
        .filter(|action_id| !action_id.trim().is_empty())
    {
        metadata.push(format!("actionId={action_id}"));
    }

    let execution_id = response_json
        .get("meta")
        .and_then(|value| value.get("executionId"))
        .or_else(|| response_json.get("executionId"))
        .and_then(|value| value.as_str())
        .filter(|execution_id| !execution_id.trim().is_empty());
    if let Some(execution_id) = execution_id {
        metadata.push(format!("executionId={execution_id}"));
    }

    if metadata.is_empty() {
        Some(message.to_owned())
    } else {
        Some(format!("{message} ({})", metadata.join(", ")))
    }
}

fn parse_connector_outputs(
    action_id: &str,
    response_json: serde_json::Value,
    outputs_def: Option<&OutputHandles>,
) -> Result<HashMap<HandleName, serde_json::Value>> {
    let success = response_json
        .get("success")
        .and_then(|value| value.as_bool())
        .ok_or_else(|| {
            utils::error::Error::new(&format!(
                "Connector action '{action_id}' response is missing success field"
            ))
        })?;

    if !success {
        let message =
            format_connector_error_details(&response_json).unwrap_or_else(|| "unknown error".to_owned());
        return Err(utils::error::Error::new(&format!(
            "Connector action '{action_id}' failed: {message}"
        )));
    }

    let data = response_json.get("data").ok_or_else(|| {
        utils::error::Error::new(&format!(
            "Connector action '{action_id}' response is missing data field"
        ))
    })?;

    if connector_uses_single_output_handle(outputs_def) {
        return Ok(HashMap::from([(HandleName::from("output"), data.clone())]));
    }

    let outputs = data.as_object().ok_or_else(|| {
        utils::error::Error::new(&format!(
            "Connector action '{action_id}' response data field must be an object"
        ))
    })?;

    Ok(outputs
        .iter()
        .map(|(handle, value)| (HandleName::from(handle.clone()), value.clone()))
        .collect())
}

fn connector_uses_single_output_handle(outputs_def: Option<&OutputHandles>) -> bool {
    outputs_def.is_some_and(|outputs| {
        outputs.len() == 1 && outputs.contains_key(&HandleName::from("output"))
    })
}

fn spawn(
    spawn_options: &SpawnOptions,
    dir: &str,
    address: &str,
    session_id: &SessionId,
    job_id: &JobId,
) -> Result<process::Child> {
    let mut args = spawn_options
        .args
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<&str>>();

    // add block task arguments
    args.extend([
        "--address",
        address,
        "--session-id",
        session_id,
        "--job-id",
        job_id,
    ]);

    // Execute the command
    let mut command = process::Command::new(&spawn_options.bin);

    command.current_dir(dir);

    command
        .args(args)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            let program = command.get_program().to_string_lossy().to_string();
            let current_dir = match command.get_current_dir() {
                Some(cwd) => to_absolute(cwd),
                None => ".".to_owned(),
            };

            utils::error::Error::with_source(
                &format!(
                    "Failed to execute '{} {} <...OOCANA_ARGS>' at '{}'",
                    program,
                    spawn_options.args.join(" "),
                    current_dir,
                ),
                Box::new(e),
            )
        })
}

fn bind_shell_stdio(
    child: &mut tokio::process::Child,
    reporter: &Arc<BlockReporterTx>,
    scheduler_tx: SchedulerTx,
    session_id: &SessionId,
    job_id: JobId,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();

    if let Some(stdout) = child.stdout.take() {
        let reporter = Arc::clone(reporter);
        let job_id_clone = job_id.clone();
        let scheduler_tx_clone = scheduler_tx.clone();
        let session_id_clone = session_id.clone();

        handles.push(tokio::spawn(async move {
            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut output = String::new();
            while let Some(line) = stdout_reader.next_line().await.unwrap_or(None) {
                reporter.log(&line, "stdout");
                output.push_str(&line);
                output.push('\n');
            }

            if output.ends_with('\n') {
                output.pop();
            }
            scheduler_tx_clone.send_block_event(scheduler::ReceiveMessage::BlockOutput {
                session_id: session_id_clone,
                job_id: job_id_clone.clone(),
                output: serde_json::json!(output),
                handle: "stdout".to_string().into(),
                options: None,
            });
        }));
    }

    if let Some(stderr) = child.stderr.take() {
        let reporter = Arc::clone(reporter);
        let job_id_clone = job_id;
        let scheduler_tx_clone = scheduler_tx.clone();
        let session_id_clone = session_id.clone();

        handles.push(tokio::spawn(async move {
            let mut stderr_reader = BufReader::new(stderr).lines();
            let mut stderr_output = String::new();
            while let Some(line) = stderr_reader.next_line().await.unwrap_or(None) {
                reporter.log(&line, "stderr");
                stderr_output.push_str(&line);
                stderr_output.push('\n');
            }
            if stderr_output.ends_with('\n') {
                stderr_output.pop();
            }
            scheduler_tx_clone.send_block_event(scheduler::ReceiveMessage::BlockOutput {
                session_id: session_id_clone.clone(),
                job_id: job_id_clone,
                output: serde_json::json!(stderr_output),
                handle: "stderr".to_string().into(),
                options: None,
            });
        }));
    }

    handles
}

fn bind_stdio(
    child: &mut process::Child,
    reporter: &Arc<BlockReporterTx>,
    spawn_handles: &mut Vec<tokio::task::JoinHandle<()>>,
) {
    if let Some(stdout) = child.stdout.take() {
        if let Ok(async_stdout) = tokio::process::ChildStdout::from_std(stdout) {
            let mut stdout_reader = BufReader::new(async_stdout).lines();
            let reporter = Arc::clone(reporter);
            spawn_handles.push(tokio::spawn(async move {
                while let Some(line) = stdout_reader.next_line().await.unwrap_or(None) {
                    reporter.log(&line, "stdout");
                }
            }));
        }
    }

    if let Some(stderr) = child.stderr.take() {
        if let Ok(async_stderr) = tokio::process::ChildStderr::from_std(stderr) {
            let mut stderr_reader = BufReader::new(async_stderr).lines();
            let reporter = Arc::clone(reporter);
            spawn_handles.push(tokio::spawn(async move {
                while let Some(line) = stderr_reader.next_line().await.unwrap_or(None) {
                    reporter.log(&line, "stderr");
                }
            }));
        }
    }
}

pub fn timeout_abort(
    job_id: JobId,
    timeout: std::time::Duration,
    block_status: BlockStatusTx,
    reporter: Arc<BlockReporterTx>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        reporter.error(&format!("{job_id} timeout after {timeout:?}"));
        block_status.finish(job_id, None, Some("Timeout".to_owned()), None);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use utils::output::OutputValue;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    #[tokio::test]
    async fn connector_executor_posts_inputs_and_reads_data_as_outputs() {
        let _env_guard = crate::CONNECTOR_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();
        let mock_server = MockServer::start().await;
        let previous_connector_base_url = std::env::var_os(CONNECTOR_BASE_URL_ENV_KEY);
        let previous_oomol_token = std::env::var_os("OOMOL_TOKEN");

        let mut inputs = BlockInputs::new();
        inputs.insert(
            HandleName::from("message"),
            Arc::new(OutputValue::new(serde_json::json!("hello"), true)),
        );
        inputs.insert(
            HandleName::from("count"),
            Arc::new(OutputValue::new(serde_json::json!(3), true)),
        );

        Mock::given(method("POST"))
            .and(path("/v1/actions/run-action"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "input": {
                    "message": "hello",
                    "count": 3
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "message": "ok",
                "data": {
                    "result": "ok",
                    "total": 4
                },
                "meta": {
                    "executionId": "exec-1",
                    "actionId": "run-action"
                }
            })))
            .mount(&mock_server)
            .await;

        unsafe {
            std::env::set_var(CONNECTOR_BASE_URL_ENV_KEY, mock_server.uri());
            std::env::set_var("OOMOL_TOKEN", "test-token");
        }

        let outputs =
            run_connector_action("run-action", Some(inputs), None, Duration::from_secs(30)).await;

        unsafe {
            if let Some(value) = previous_connector_base_url {
                std::env::set_var(CONNECTOR_BASE_URL_ENV_KEY, value);
            } else {
                std::env::remove_var(CONNECTOR_BASE_URL_ENV_KEY);
            }
            if let Some(value) = previous_oomol_token {
                std::env::set_var("OOMOL_TOKEN", value);
            } else {
                std::env::remove_var("OOMOL_TOKEN");
            }
        }

        let outputs = outputs.unwrap();

        assert_eq!(
            outputs.get(&HandleName::from("result")),
            Some(&serde_json::json!("ok"))
        );
        assert_eq!(
            outputs.get(&HandleName::from("total")),
            Some(&serde_json::json!(4))
        );
    }

    #[tokio::test]
    async fn connector_executor_requires_object_data() {
        let error = parse_connector_outputs(
            "run-action",
            serde_json::json!({
                "success": true,
                "message": "ok",
                "data": 1,
                "meta": {
                    "executionId": "exec-1",
                    "actionId": "run-action"
                }
            }),
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' response data field must be an object"
        );
    }

    #[tokio::test]
    async fn connector_executor_requires_data_field() {
        let error = parse_connector_outputs(
            "run-action",
            serde_json::json!({
                "success": true,
                "message": "ok",
                "meta": {
                    "executionId": "exec-1",
                    "actionId": "run-action"
                }
            }),
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' response is missing data field"
        );
    }

    #[tokio::test]
    async fn connector_executor_reports_unsuccessful_response_message() {
        let error = parse_connector_outputs(
            "run-action",
            serde_json::json!({
                "success": false,
                "message": "invalid auth token",
                "data": {
                    "ignored": true
                }
            }),
            None,
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' failed: invalid auth token"
        );
    }

    #[tokio::test]
    async fn connector_executor_reports_non_success_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/actions/run-action"))
            .respond_with(ResponseTemplate::new(500).set_body_string("connector failed"))
            .mount(&mock_server)
            .await;

        let error = run_connector_action_with_base_url(
            &mock_server.uri(),
            "run-action",
            None,
            None,
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' failed with status 500 Internal Server Error: connector failed"
        );
    }

    #[tokio::test]
    async fn connector_executor_reports_structured_4xx_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/actions/run-action"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "success": false,
                "message": "input validation failed",
                "data": null,
                "errorCode": "invalid_input",
                "meta": {
                    "actionId": "run-action",
                    "executionId": "exec-400"
                }
            })))
            .mount(&mock_server)
            .await;

        let error = run_connector_action_with_base_url(
            &mock_server.uri(),
            "run-action",
            None,
            None,
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' failed with status 400 Bad Request: input validation failed (errorCode=invalid_input, actionId=run-action, executionId=exec-400)"
        );
    }

    #[tokio::test]
    async fn connector_executor_reports_structured_401_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/actions/run-action"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errorCode": "invalid_input",
                "errorMessage": "missing authorization header",
                "executionId": "exec-401"
            })))
            .mount(&mock_server)
            .await;

        let error = run_connector_action_with_base_url(
            &mock_server.uri(),
            "run-action",
            None,
            None,
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' failed with status 401 Unauthorized: missing authorization header (errorCode=invalid_input, executionId=exec-401)"
        );
    }

    #[tokio::test]
    async fn connector_executor_reports_structured_5xx_status() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/actions/run-action"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "success": false,
                "message": "provider unavailable",
                "data": null,
                "errorCode": "provider_error",
                "meta": {
                    "actionId": "run-action",
                    "executionId": "exec-500"
                }
            })))
            .mount(&mock_server)
            .await;

        let error = run_connector_action_with_base_url(
            &mock_server.uri(),
            "run-action",
            None,
            None,
            Duration::from_secs(30),
        )
        .await
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Connector action 'run-action' failed with status 500 Internal Server Error: provider unavailable (errorCode=provider_error, actionId=run-action, executionId=exec-500)"
        );
    }


    #[test]
    fn connector_executor_detects_single_output_handle() {
        assert!(connector_uses_single_output_handle(Some(&HashMap::from([(
            HandleName::from("output"),
            manifest_reader::manifest::OutputHandle {
                handle: HandleName::from("output"),
                description: None,
                json_schema: None,
                kind: None,
                nullable: None,
                is_additional: false,
                _serialize_for_cache: false,
            },
        )]))));
    }

    #[tokio::test]
    async fn connector_executor_wraps_object_data_for_single_output_handle() {
        let outputs = parse_connector_outputs(
            "run-action",
            serde_json::json!({
                "success": true,
                "message": "ok",
                "data": {
                    "bucket": "demo",
                    "count": 2
                },
                "meta": {
                    "executionId": "exec-1",
                    "actionId": "run-action"
                }
            }),
            Some(&HashMap::from([(
                HandleName::from("output"),
                manifest_reader::manifest::OutputHandle {
                    handle: HandleName::from("output"),
                    description: None,
                    json_schema: None,
                    kind: None,
                    nullable: None,
                    is_additional: false,
                    _serialize_for_cache: false,
                },
            )])),
        )
        .unwrap();

        assert_eq!(
            outputs.get(&HandleName::from("output")),
            Some(&serde_json::json!({
                "bucket": "demo",
                "count": 2
            }))
        );
    }

    #[tokio::test]
    async fn connector_executor_allows_non_object_data_for_single_output_handle() {
        let outputs = parse_connector_outputs(
            "run-action",
            serde_json::json!({
                "success": true,
                "message": "ok",
                "data": ["a", "b"],
                "meta": {
                    "executionId": "exec-1",
                    "actionId": "run-action"
                }
            }),
            Some(&HashMap::from([(
                HandleName::from("output"),
                manifest_reader::manifest::OutputHandle {
                    handle: HandleName::from("output"),
                    description: None,
                    json_schema: None,
                    kind: None,
                    nullable: None,
                    is_additional: false,
                    _serialize_for_cache: false,
                },
            )])),
        )
        .unwrap();

        assert_eq!(
            outputs.get(&HandleName::from("output")),
            Some(&serde_json::json!(["a", "b"]))
        );
    }

    #[test]
    fn connector_executor_encodes_action_id_as_single_path_segment() {
        let url = build_connector_action_url("https://connector.example/base", "a/b ?c#d").unwrap();

        assert_eq!(
            url.as_str(),
            "https://connector.example/base/v1/actions/a%2Fb%20%3Fc%23d"
        );
    }

    #[test]
    fn connector_executor_truncates_large_error_bodies() {
        let body = "x".repeat(CONNECTOR_ERROR_BODY_LIMIT + 10);
        let truncated = truncate_connector_error_body(&body);

        assert!(truncated.ends_with("...<truncated>"));
        assert!(truncated.len() > CONNECTOR_ERROR_BODY_LIMIT);
    }
}
