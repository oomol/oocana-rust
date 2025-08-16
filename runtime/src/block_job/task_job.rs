use mainframe::reporter::BlockReporterTx;
use mainframe::scheduler::{self, ExecutorParams, SchedulerTx};
use manifest_meta::{
    HandleName, InputDefPatchMap, InputHandles, OutputHandles, SubflowBlock, TaskBlock,
    TaskBlockExecutor,
};
use manifest_reader::manifest::SpawnOptions;

use std::collections::HashMap;
use std::path::Path;
use std::{process, sync::Arc};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::block_status::BlockStatusTx;
use crate::shared::Shared;

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope, SessionId};
use utils::error::Result;
use utils::path::to_absolute;

use super::job_handle::BlockJobHandle;
use super::listener::{listen_to_worker, ListenerParameters};

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
    pub task_block: Arc<TaskBlock>,
    pub inputs_def: Option<InputHandles>, // block's inputs def missing some node added inputs,
    pub outputs_def: Option<OutputHandles>, // block's outputs def will miss additional outputs added on node
    pub shared: Arc<Shared>,
    pub parent_flow: Option<Arc<SubflowBlock>>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    pub scope: RuntimeScope,
    pub timeout: Option<u64>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
}

pub fn execute_task_job(params: TaskJobParameters) -> Option<BlockJobHandle> {
    let TaskJobParameters {
        task_block,
        inputs_def,
        outputs_def,
        shared,
        parent_flow,
        stacks,
        job_id,
        inputs,
        block_status,
        scope,
        timeout,
        inputs_def_patch,
    } = params;
    let reporter = Arc::new(shared.reporter.block(
        job_id.to_owned(),
        task_block.path_str(),
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
        block_path: task_block.path_str(),
        stacks: stacks.clone(),
        scheduler_tx: shared.scheduler_tx.clone(),
        inputs: inputs.clone(),
        outputs_def: outputs_def.clone(),
        inputs_def,
        block_status: block_status.clone(),
        reporter: Arc::clone(&reporter),
        executor: Some(task_block.executor.clone()),
        service: None,
        block_dir: block_dir(&task_block, parent_flow.as_ref(), Some(&scope)),
        scope: scope.clone(),
        injection_store: parent_flow.as_ref().and_then(|f| f.injection_store.clone()),
        flow: parent_flow.as_ref().map(|f| f.path_str.clone()),
        inputs_def_patch,
    });

    match task_block.executor.clone() {
        TaskBlockExecutor::Rust(e) => {
            let execute_result = spawn(
                &task_block,
                &e.options,
                parent_flow.as_ref(),
                &shared.address,
                &shared.session_id,
                &job_id,
            );

            match execute_result {
                Ok(mut child) => {
                    spawn_handles.push(worker_listener_handle);
                    bind_stdio(&mut child, &reporter, &mut spawn_handles);

                    Some(BlockJobHandle::new(
                        job_id.to_owned(),
                        TaskJobHandle {
                            job_id,
                            shared,
                            child: Some(child),
                            spawn_handles,
                        },
                    ))
                }
                Err(e) => {
                    worker_listener_handle.abort();
                    reporter.finished(None, Some(e.to_string()));
                    Some(BlockJobHandle::new(
                        job_id.to_owned(),
                        TaskJobHandle {
                            job_id,
                            shared,
                            child: None,
                            spawn_handles,
                        },
                    ))
                }
            }
        }
        TaskBlockExecutor::Shell(_) => {
            let execute_result = spawn_shell(
                &task_block,
                parent_flow.as_ref(),
                inputs,
                &shared.session_id,
                &job_id,
            );

            match execute_result {
                Ok(mut child) => {
                    spawn_handles.push(worker_listener_handle);
                    bind_shell_stdio(
                        &mut child,
                        &reporter,
                        &mut spawn_handles,
                        shared.scheduler_tx.clone(),
                        &shared.session_id,
                        job_id.clone(),
                    );

                    let job_id_clone = job_id.clone();
                    let shared_clone = Arc::clone(&shared);

                    // TODO: consider to use tokio::process::Command
                    let exit_handler = tokio::task::spawn_blocking(move || {
                        let status = child.wait().unwrap();
                        let status_code = status.code().unwrap_or(-1);
                        if status_code != 0 {
                            let msg = format!(
                                "Task block '{:?}' exited with code {}",
                                task_block.path_str(),
                                status_code
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

                    Some(BlockJobHandle::new(
                        job_id.to_owned(),
                        TaskJobHandle {
                            job_id,
                            shared,
                            child: None, // TODO: keep child and also wait exit code
                            spawn_handles,
                        },
                    ))
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
                    Some(BlockJobHandle::new(
                        job_id.to_owned(),
                        TaskJobHandle {
                            job_id,
                            shared,
                            child: None,
                            spawn_handles,
                        },
                    ))
                }
            }
        }
        _ => {
            send_to_executor(ExecutorArgs {
                task_block: &task_block,
                executor: &task_block.executor,
                parent_flow: parent_flow.as_ref(),
                job_id: &job_id,
                scheduler_tx: shared.scheduler_tx.clone(),
                outputs_def: &outputs_def,
                scope: &scope,
                stacks,
            });

            spawn_handles.push(worker_listener_handle);

            Some(BlockJobHandle::new(
                job_id.to_owned(),
                TaskJobHandle {
                    job_id,
                    shared,
                    child: None,
                    spawn_handles,
                },
            ))
        }
    }
}

fn block_dir(
    task_block: &TaskBlock,
    parent_flow: Option<&Arc<SubflowBlock>>,
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
                f.path
                    .parent()
                    .map(|p| p.to_str().unwrap_or(".").to_owned())
            })
            .unwrap_or(".".to_owned())
    }
}

struct ExecutorArgs<'a> {
    task_block: &'a TaskBlock,
    executor: &'a TaskBlockExecutor,
    outputs_def: &'a Option<OutputHandles>,
    parent_flow: Option<&'a Arc<SubflowBlock>>,
    job_id: &'a JobId,
    scope: &'a RuntimeScope,
    scheduler_tx: SchedulerTx,
    stacks: BlockJobStacks,
}

fn send_to_executor(args: ExecutorArgs) {
    let ExecutorArgs {
        task_block,
        executor,
        parent_flow,
        job_id,
        scheduler_tx,
        outputs_def,
        scope,
        stacks,
    } = args;
    let dir = block_dir(task_block, parent_flow, Some(scope));
    scheduler_tx.send_to_executor(ExecutorParams {
        executor_name: executor.name(),
        job_id: job_id.to_owned(),
        stacks: stacks.vec(),
        dir,
        executor,
        outputs: outputs_def,
        scope,
        injection_store: &parent_flow.as_ref().and_then(|f| f.injection_store.clone()),
        flow: &parent_flow.as_ref().map(|f| f.path_str.clone()),
    })
}

fn spawn_shell(
    task_block: &TaskBlock,
    parent_flow: Option<&Arc<SubflowBlock>>,
    inputs: Option<BlockInputs>,
    session_id: &SessionId,
    job_id: &JobId,
) -> Result<process::Child> {
    let mut envs = HashMap::new();
    envs.insert("OOCANA_SESSION_ID".to_string(), session_id.to_string());
    envs.insert("OOCANA_JOB_ID".to_string(), job_id.to_string());

    let arg = get_string_value_from_inputs(&inputs, "command");

    let block_dir = block_dir(task_block, parent_flow, None);

    // 用户设置 cwd 在这里的意义不大，造成的问题反而可能更多，考虑去掉。
    let cwd = match get_string_value_from_inputs(&inputs, "cwd") {
        Some(cwd) => {
            let path = std::path::Path::new(&cwd);
            if path.is_relative() {
                std::path::Path::new(&block_dir)
                    .join(path)
                    .to_str()
                    .unwrap()
                    .to_owned()
            } else {
                cwd
            }
        }
        None => block_dir,
    };

    let env_strings = get_string_value_from_inputs(&inputs, "envs");
    if let Some(env_str) = env_strings {
        let env_key_value_pairs: Vec<&str> = env_str.split(',').collect();
        for env in env_key_value_pairs {
            let key_value: Vec<&str> = env.split('=').collect();
            if key_value.len() == 2 {
                envs.insert(key_value[0].to_string(), key_value[1].to_string());
            }
        }
    }

    let mut command = process::Command::new("sh");

    // 如果是相对链接，根据 block_dir 来处理相对地址。rust 如果使用 canonicalize 如果文件不存在会直接报错
    let canonicalize = Path::new(&cwd).canonicalize();
    match canonicalize {
        Ok(cwd) => {
            command.current_dir(cwd);
        }
        Err(err) => {
            return Err(utils::error::Error::new(&format!(
                "Failed to canonicalize cwd {} with error: {}",
                cwd, err
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
            let program = command.get_program().to_string_lossy().to_string();
            let current_dir = match command.get_current_dir() {
                Some(cwd) => to_absolute(cwd),
                None => ".".to_owned(),
            };

            utils::error::Error::with_source(
                &format!(
                    "Failed to execute '{} {} <...OOCANA_ARGS>' at '{}'",
                    program,
                    reporter_err.unwrap_or_default(),
                    current_dir,
                ),
                Box::new(e),
            )
        })
}

fn get_string_value_from_inputs(inputs: &Option<BlockInputs>, key: &str) -> Option<String> {
    match inputs {
        Some(inputs) => {
            let key = HandleName::new(key.to_string());
            let v = inputs.get(&key);
            if v.is_some_and(|v| v.value.is_string()) {
                Some(v.unwrap().value.as_str().unwrap().to_owned())
            } else {
                None
            }
        }
        None => None,
    }
}

fn spawn(
    task_block: &TaskBlock,
    spawn_options: &SpawnOptions,
    parent_flow: Option<&Arc<SubflowBlock>>,
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

    let block_dir = block_dir(task_block, parent_flow, None);
    command.current_dir(block_dir);

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
    child: &mut process::Child,
    reporter: &Arc<BlockReporterTx>,
    spawn_handles: &mut Vec<tokio::task::JoinHandle<()>>,
    scheduler_tx: SchedulerTx,
    session_id: &SessionId,
    job_id: JobId,
) {
    if let Some(stdout) = child.stdout.take() {
        if let Ok(async_stdout) = tokio::process::ChildStdout::from_std(stdout) {
            let reporter = Arc::clone(reporter);
            let job_id_clone = job_id.clone();
            let scheduler_tx_clone = scheduler_tx.clone();
            let session_id_clone = session_id.clone();

            spawn_handles.push(tokio::spawn(async move {
                let mut stdout_reader = BufReader::new(async_stdout).lines();
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
    }

    if let Some(stderr) = child.stderr.take() {
        if let Ok(async_stderr) = tokio::process::ChildStderr::from_std(stderr) {
            let reporter = Arc::clone(reporter);
            let job_id_clone = job_id;
            let scheduler_tx_clone = scheduler_tx.clone();
            let session_id_clone = session_id.clone();

            spawn_handles.push(tokio::spawn(async move {
                let mut stderr_reader = BufReader::new(async_stderr).lines();
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
    }
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
        reporter.error(&format!("{} timeout after {:?}", job_id, timeout));
        block_status.finish(job_id, None, Some("Timeout".to_owned()));
    })
}
