use mainframe::reporter::BlockReporterTx;
use mainframe::scheduler::{ExecutorParams, SchedulerTx};
use manifest_meta::{FlowBlock, HandleName, InputDefPatchMap, TaskBlock, TaskBlockExecutor};
use manifest_reader::manifest::SpawnOptions;

use std::collections::HashMap;
use std::path::Path;
use std::{process, sync::Arc};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::block_status::BlockStatusTx;
use crate::shared::Shared;
use utils::output::OutputValue;

use job::{BlockInputs, BlockJobStacks, JobId, SessionId};
use utils::error::Result;
use utils::path::to_absolute;

use super::block::BlockJobHandle;
use super::listener::{listen_to_worker, ListenerArgs};

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

pub struct RunTaskBlockArgs {
    pub task_block: Arc<TaskBlock>,
    pub shared: Arc<Shared>,
    pub parent_flow: Option<Arc<FlowBlock>>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    pub timeout_seconds: Option<u64>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
}

pub fn run_task_block(args: RunTaskBlockArgs) -> Option<BlockJobHandle> {
    let RunTaskBlockArgs {
        task_block,
        shared,
        parent_flow,
        stacks,
        job_id,
        inputs,
        block_status,
        timeout_seconds,
        inputs_def_patch,
    } = args;
    let reporter = Arc::new(shared.reporter.block(
        job_id.to_owned(),
        task_block.path_str.clone(),
        stacks.clone(),
    ));

    reporter.started(&inputs);

    let mut spawn_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    if let Some(timeout_seconds) = timeout_seconds {
        let timeout_handle = timeout_abort(
            job_id.to_owned(),
            std::time::Duration::from_secs(timeout_seconds),
            block_status.clone(),
            Arc::clone(&reporter),
        );

        spawn_handles.push(timeout_handle);
    }

    let worker_listener_handle = listen_to_worker(ListenerArgs {
        job_id: job_id.to_owned(),
        block_path: task_block.path_str.clone(),
        stacks: stacks.clone(),
        scheduler_tx: shared.scheduler_tx.clone(),
        inputs: inputs.clone(),
        outputs_def: task_block.outputs_def.clone(),
        inputs_def: task_block.inputs_def.clone(),
        block_status: block_status.clone(),
        reporter: Arc::clone(&reporter),
        executor: task_block.executor.clone(),
        service: None,
        block_dir: block_dir(&task_block, parent_flow.as_ref()),
        package_path: task_block.package_path.clone(),
        injection_store: parent_flow.as_ref().and_then(|f| f.injection_store.clone()),
        flow: parent_flow.as_ref().map(|f| f.path_str.clone()),
        inputs_def_patch,
    });

    if let Some(executor) = &task_block.executor {
        match executor {
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
                        reporter.done(&Some(e.to_string()));
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
                        // TODO: shell executor 不需要 listen worker
                        let block_status_clone = block_status.clone();
                        spawn_handles.push(worker_listener_handle);
                        bind_shell_stdio(
                            &mut child,
                            &reporter,
                            &mut spawn_handles,
                            block_status,
                            job_id.clone(),
                        );

                        let job_id_clone = job_id.clone();

                        // TODO: consider to use tokio::process::Command
                        let exit_handler = tokio::task::spawn_blocking(move || {
                            let status = child.wait().unwrap();
                            let status_code = status.code().unwrap_or(-1);
                            if status_code != 0 {
                                block_status_clone.done(
                                    job_id_clone.clone(),
                                    Some(format!("Exit code: {}", status_code)),
                                );
                                reporter.done(&Some(format!("Exit code: {}", status_code)));
                            } else {
                                block_status_clone.done(job_id_clone.clone(), None);
                                reporter.done(&None);
                            }
                        });

                        spawn_handles.push(exit_handler);

                        return Some(BlockJobHandle::new(
                            job_id.to_owned(),
                            TaskJobHandle {
                                job_id,
                                shared,
                                child: None, // TODO: keep child and also wait exit code
                                spawn_handles,
                            },
                        ));
                    }
                    Err(e) => {
                        block_status.done(job_id.clone(), Some(e.to_string()));
                        worker_listener_handle.abort();
                        reporter.done(&Some(e.to_string()));
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
                    executor,
                    parent_flow: parent_flow.as_ref(),
                    job_id: &job_id,
                    scheduler_tx: shared.scheduler_tx.clone(),
                    stacks,
                });

                spawn_handles.push(worker_listener_handle);

                return Some(BlockJobHandle::new(
                    job_id.to_owned(),
                    TaskJobHandle {
                        job_id,
                        shared,
                        child: None,
                        spawn_handles,
                    },
                ));
            }
        }
    } else {
        reporter.done(&Some("No executor or entry found".to_owned()));
        None
    }
}

fn block_dir(task_block: &TaskBlock, parent_flow: Option<&Arc<FlowBlock>>) -> String {
    if let Some(block_dir) = task_block.block_dir() {
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
    parent_flow: Option<&'a Arc<FlowBlock>>,
    job_id: &'a JobId,
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
        stacks,
    } = args;
    let dir = block_dir(task_block, parent_flow);
    scheduler_tx.send_to_executor(ExecutorParams {
        executor_name: executor.name(),
        job_id: job_id.to_owned(),
        stacks: stacks.vec(),
        dir,
        executor: &executor,
        outputs: &task_block.outputs_def,
        package_path: &task_block.package_path,
        injection_store: &parent_flow.as_ref().and_then(|f| f.injection_store.clone()),
        flow: &parent_flow.as_ref().map(|f| f.path_str.clone()),
    })
}

fn spawn_shell(
    task_block: &TaskBlock,
    parent_flow: Option<&Arc<FlowBlock>>,
    inputs: Option<BlockInputs>,
    session_id: &SessionId,
    job_id: &JobId,
) -> Result<process::Child> {
    let mut envs = HashMap::new();
    envs.insert("OOCANA_SESSION_ID".to_string(), session_id.to_string());
    envs.insert("OOCANA_JOB_ID".to_string(), job_id.to_string());

    let arg = get_string_value_from_inputs(&inputs, "command");

    let block_dir = block_dir(task_block, parent_flow);

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
    parent_flow: Option<&Arc<FlowBlock>>,
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
    args.extend(
        [
            "--address",
            address,
            "--session-id",
            session_id,
            "--job-id",
            job_id,
        ]
        .into_iter(),
    );

    // Execute the command
    let mut command = process::Command::new(&spawn_options.bin);

    let block_dir = block_dir(task_block, parent_flow);
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
    block_status: BlockStatusTx,
    job_id: JobId,
) {
    if let Some(stdout) = child.stdout.take() {
        if let Ok(async_stdout) = tokio::process::ChildStdout::from_std(stdout) {
            let reporter = Arc::clone(reporter);
            let job_id_clone = job_id.clone();
            let block_status_clone = block_status.clone();
            spawn_handles.push(tokio::spawn(async move {
                let mut stdout_reader = BufReader::new(async_stdout).lines();
                let mut output = String::new();
                while let Some(line) = stdout_reader.next_line().await.unwrap_or(None) {
                    reporter.log(&line, "stdout");
                    output.push_str(&line);
                    output.push_str("\n");
                }

                if output.ends_with('\n') {
                    output.pop();
                }
                block_status_clone.result(
                    job_id_clone.clone(),
                    Arc::new(OutputValue {
                        value: serde_json::json!(output),
                        cacheable: true,
                    }),
                    "stdout".to_string().into(),
                    true,
                );
            }));
        }
    }

    if let Some(stderr) = child.stderr.take() {
        if let Ok(async_stderr) = tokio::process::ChildStderr::from_std(stderr) {
            let reporter = Arc::clone(reporter);
            let job_id_clone = job_id.clone();
            spawn_handles.push(tokio::spawn(async move {
                let mut stderr_reader = BufReader::new(async_stderr).lines();
                let mut stderr_output = String::new();
                while let Some(line) = stderr_reader.next_line().await.unwrap_or(None) {
                    reporter.log(&line, "stderr");
                    stderr_output.push_str(&line);
                    stderr_output.push_str("\n");
                }
                if stderr_output.ends_with('\n') {
                    stderr_output.pop();
                }

                block_status.result(
                    job_id_clone.clone(),
                    Arc::new(OutputValue {
                        value: serde_json::json!(stderr_output),
                        cacheable: true,
                    }),
                    "stderr".to_string().into(),
                    true,
                );
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
        reporter.error(&format!(
            "{} timeout after {:?}",
            job_id.to_string(),
            timeout
        ));
        block_status.done(job_id, Some("Timeout".to_owned()));
    })
}
