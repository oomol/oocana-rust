use mainframe::scheduler::SchedulerTx;
use mainframe::{reporter::BlockReporterTx, worker};
use manifest_meta::{
    FlowBlock, InputHandles, OutputHandles, TaskBlock, TaskBlockEntry, TaskBlockExecutor,
    OOMOL_VAR_DATA,
};
use serde_json::Value;
use std::{process, sync::Arc};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::BlockJobHandle;
use crate::block_status::BlockStatusTx;
use crate::shared::Shared;
use utils::output::{self, OutputValue};

use job::{BlockInputs, BlockJobStacks, JobId, SessionId};
use utils::error::Result;
use utils::path::to_absolute;

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

pub fn run_task_block(
    task_block: Arc<TaskBlock>, shared: Arc<Shared>, parent_flow: Option<Arc<FlowBlock>>,
    stacks: BlockJobStacks, job_id: JobId, inputs: Option<BlockInputs>,
    block_status: BlockStatusTx,
) -> Option<BlockJobHandle> {
    let reporter = Arc::new(shared.reporter.block(
        job_id.to_owned(),
        task_block.path_str.clone(),
        stacks.clone(),
    ));

    reporter.started(&inputs);

    let mut spawn_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    let worker_listener_handle = listen_to_worker(
        job_id.to_owned(),
        task_block.path_str.clone(),
        stacks.clone(),
        shared.scheduler_tx.clone(),
        inputs,
        task_block.outputs_def.clone(),
        task_block.inputs_def.clone(),
        block_status,
        Arc::clone(&reporter),
    );

    if let Some(entry) = &task_block.entry {
        let execute_result = spawn(
            &task_block,
            entry,
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
                None
            }
        }
    } else if let Some(executor) = &task_block.executor {
        send_to_executor(
            &task_block,
            executor,
            parent_flow.as_ref(),
            &job_id,
            shared.scheduler_tx.clone(),
            stacks,
        );

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
    } else {
        reporter.done(&Some("No executor or entry found".to_owned()));
        None
    }
}

fn send_to_executor(
    task_block: &TaskBlock, executor: &TaskBlockExecutor, parent_flow: Option<&Arc<FlowBlock>>,
    job_id: &JobId, scheduler_tx: SchedulerTx, stacks: BlockJobStacks,
) {
    let dir = match &task_block.path {
        Some(block_path) => block_path.parent().unwrap().to_str().unwrap().to_owned(),
        None => match parent_flow {
            Some(parent_flow) => parent_flow
                .path
                .parent()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
            None => ".".to_owned(),
        },
    };

    scheduler_tx.send_to_executor(
        executor.name.to_owned(),
        job_id.to_owned(),
        stacks.vec(),
        dir,
        executor,
        &task_block.outputs_def,
    );
}

fn spawn(
    task_block: &TaskBlock, entry: &TaskBlockEntry, parent_flow: Option<&Arc<FlowBlock>>,
    address: &str, session_id: &SessionId, job_id: &JobId,
) -> Result<process::Child> {
    let mut args = entry.args.iter().map(AsRef::as_ref).collect::<Vec<&str>>();

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
    let mut command = process::Command::new(&entry.bin);

    if let Some(cwd) = entry.cwd.as_ref() {
        command.current_dir(cwd);
    } else if let Some(block_path) = &task_block.path {
        if let Some(cwd) = block_path.parent().unwrap().to_str() {
            command.current_dir(cwd);
        }
    } else if let Some(parent_flow) = parent_flow {
        if let Some(cwd) = parent_flow.path.parent().unwrap().to_str() {
            command.current_dir(cwd);
        }
    }

    command
        .args(args)
        .envs(&entry.envs)
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
                    entry.args.join(" "),
                    current_dir,
                ),
                Box::new(e),
            )
        })
}

fn bind_stdio(
    child: &mut process::Child, reporter: &Arc<BlockReporterTx>,
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

pub fn listen_to_worker(
    job_id: JobId, block_path: Option<String>, stacks: BlockJobStacks, scheduler_tx: SchedulerTx,
    mut inputs: Option<BlockInputs>, outputs_def: Option<OutputHandles>,
    inputs_def: Option<InputHandles>, block_status: BlockStatusTx, reporter: Arc<BlockReporterTx>,
) -> tokio::task::JoinHandle<()> {
    let (job_tx, job_rx) = flume::unbounded::<worker::MessageDeserialize>();
    scheduler_tx.register_subscriber(job_id.to_owned(), job_tx);
    tokio::spawn(async move {
        while let Ok(message) = job_rx.recv_async().await {
            match message {
                worker::MessageDeserialize::BlockReady { job_id, .. } => {
                    scheduler_tx.send_inputs(
                        job_id,
                        &block_path,
                        stacks.vec(),
                        inputs.take().as_ref(),
                        &inputs_def,
                    );
                }
                worker::MessageDeserialize::BlockOutput {
                    output: result,
                    handle,
                    done,
                    job_id,
                    ..
                } => {
                    reporter.result(&result, &handle, done);

                    let sender = outputs_def.as_ref().and_then(|outputs| {
                        outputs.get(&handle).and_then(|output| {
                            output
                                .json_schema
                                .as_ref()
                                .and_then(|f: &serde_json::Value| match f {
                                    Value::Object(obj) => {
                                        obj.get("contentMediaType").as_ref().and_then(|k| {
                                            if k.to_string() == OOMOL_VAR_DATA {
                                                let s: Box<dyn output::DropSender> =
                                                    Box::new(scheduler_tx.clone());
                                                Some(Arc::new(s))
                                            } else {
                                                None
                                            }
                                        })
                                    }
                                    _ => None,
                                })
                        })
                    });
                    block_status.result(
                        job_id,
                        Arc::new(OutputValue {
                            value: result,
                            sender: sender,
                        }),
                        handle,
                        done,
                    );
                }
                worker::MessageDeserialize::BlockFinished { error, job_id, .. } => {
                    reporter.done(&error);
                    if let Some(error) = error {
                        block_status.done(job_id, Some(error));
                    } else {
                        block_status.done(job_id, None);
                    }
                }
                worker::MessageDeserialize::BlockError { error, .. } => {
                    reporter.error(&error);
                }
            }
        }
    })
}
