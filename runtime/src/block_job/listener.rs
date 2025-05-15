use std::sync::Arc;

use job::{BlockInputs, BlockJobStacks, JobId, RunningPackageScope};
use mainframe::{
    reporter::BlockReporterTx,
    scheduler::{self, ExecutorParams, SchedulerTx, ServiceParams},
};
use manifest_meta::{
    InjectionStore, InputDefPatchMap, InputHandles, OutputHandles, ServiceExecutorOptions,
    TaskBlockExecutor, OOMOL_BIN_DATA, OOMOL_SECRET_DATA, OOMOL_VAR_DATA,
};
use serde_json::Value;
use tracing::{info, warn};
use utils::output::OutputValue;

use crate::block_status::BlockStatusTx;

#[derive(Debug)]
pub struct ServiceExecutorPayload {
    pub block_name: String,
    pub options: ServiceExecutorOptions,
    pub executor_name: String,
}

pub struct ListenerArgs {
    pub job_id: JobId,
    pub block_path: Option<String>,
    pub stacks: BlockJobStacks,
    pub scheduler_tx: SchedulerTx,
    pub inputs: Option<BlockInputs>,
    pub outputs_def: Option<OutputHandles>,
    pub inputs_def: Option<InputHandles>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
    pub block_status: BlockStatusTx,
    pub reporter: Arc<BlockReporterTx>,
    pub executor: Option<TaskBlockExecutor>,
    pub service: Option<ServiceExecutorPayload>,
    pub block_dir: String,
    pub scope: RunningPackageScope,
    pub injection_store: Option<InjectionStore>,
    pub flow: Option<String>,
}

pub fn listen_to_worker(args: ListenerArgs) -> tokio::task::JoinHandle<()> {
    let ListenerArgs {
        job_id,
        block_path,
        stacks,
        scheduler_tx,
        mut inputs,
        outputs_def,
        inputs_def,
        inputs_def_patch,
        block_status,
        reporter,
        executor,
        service,
        block_dir,
        scope,
        injection_store,
        flow,
    } = args;

    let block_scope = scheduler_tx.calculate_scope(&scope);

    let (job_tx, job_rx) = flume::unbounded::<scheduler::ReceiveMessage>();

    scheduler_tx.register_subscriber(job_id.to_owned(), job_tx);
    tokio::spawn(async move {
        let mut has_executor_response = false;
        while let Ok(message) = job_rx.recv_async().await {
            match message {
                scheduler::ReceiveMessage::ExecutorReady {
                    executor_name,
                    package: executor_package,
                    identifier,
                    ..
                } => {
                    tracing::info!("{executor_name} {identifier:?} ({executor_package:?}) executor is ready. block package: {block_scope:?}");

                    if identifier
                        .as_ref()
                        .is_none_or(|id| id != &block_scope.identifier())
                    {
                        info!(
                            "executor {} identifier {:?} is not equal to block identifier {:?}",
                            executor_name,
                            identifier,
                            block_scope.identifier()
                        );
                        continue;
                    }

                    if let Some(ref executor) = executor {
                        if executor_name != executor.name() {
                            info!(
                                "executor {} is not equal to block executor {}",
                                executor_name,
                                executor.name()
                            );
                            continue;
                        }

                        scheduler_tx.send_to_executor(ExecutorParams {
                            executor_name: &executor_name,
                            job_id: job_id.to_owned(),
                            stacks: stacks.vec(),
                            dir: block_dir.to_owned(),
                            executor,
                            outputs: &outputs_def,
                            scope: &scope,
                            injection_store: &injection_store,
                            flow: &flow,
                        });
                    } else if let Some(ref service) = service {
                        if executor_name != service.executor_name {
                            info!(
                                "executor {} is not equal to block executor {}",
                                executor_name, service.executor_name
                            );
                            continue;
                        }

                        scheduler_tx.send_to_service(ServiceParams {
                            executor_name: &executor_name,
                            block_name: &service.block_name,
                            job_id: job_id.to_owned(),
                            stacks: stacks.vec(),
                            dir: block_dir.to_owned(),
                            options: &service.options,
                            outputs: &outputs_def,
                            scope: &scope,
                            flow: &flow,
                        });
                    }
                }
                scheduler::ReceiveMessage::ExecutorExit {
                    executor_name,
                    code,
                    reason,
                    ..
                } => {
                    let msg = reason.unwrap_or(format!(
                        "Executor {} exit with code {}",
                        executor_name, code
                    ));

                    reporter.done(&Some(msg.to_owned()));
                    block_status.error(msg);
                }
                scheduler::ReceiveMessage::ExecutorTimeout {
                    executor_name,
                    package,
                    identifier,
                    ..
                } => {
                    if identifier
                        .as_ref()
                        .is_none_or(|id| id != &block_scope.identifier())
                    {
                        info!(
                            "executor {} identifier {:?} is not equal to block identifier {:?}",
                            executor_name,
                            identifier,
                            block_scope.identifier()
                        );
                        continue;
                    }

                    let error_message = format!(
                        "Executor {} identifier {:?} for package {:?} timeout after 5s",
                        executor_name, identifier, package
                    );

                    block_status.error(error_message.clone());
                    reporter.done(&Some(error_message));
                }
                scheduler::ReceiveMessage::BlockReady { job_id, .. } => {
                    has_executor_response = true;
                    scheduler_tx.send_inputs(scheduler::InputParams {
                        job_id: job_id.to_owned(),
                        block_path: block_path.clone(),
                        stacks: stacks.vec().clone(),
                        inputs: inputs.take().clone(),
                        inputs_def: inputs_def.clone(),
                        inputs_def_patch: inputs_def_patch.clone(),
                    });
                }
                scheduler::ReceiveMessage::ListenerTimeout {
                    job_id: msg_job_id, ..
                } => {
                    if has_executor_response || job_id != msg_job_id {
                        continue;
                    }
                    // 用户 block 可能会耗尽资源，导致无法及时响应。暂时只记录日志，不做处理
                    // let error_message =
                    // Some(format!("executor execute {msg_job_id} timeout after 10s"));
                    warn!("listener wait timeout 10s. job_id: {msg_job_id}");
                    // reporter.done(&error_message);
                    // block_status.error(error_message.unwrap_or_default());
                }
                scheduler::ReceiveMessage::BlockOutput {
                    output: value,
                    handle,
                    done,
                    job_id,
                    ..
                } => {
                    reporter.output(&value, &handle, done);

                    let mut cacheable = outputs_def
                        .as_ref()
                        .and_then(|outputs| outputs.get(&handle))
                        .and_then(|output| output.json_schema.as_ref())
                        .and_then(|schema| match schema {
                            Value::Object(obj) => {
                                obj.get("contentMediaType")
                                    .map(|media_type| match media_type {
                                        Value::String(t) => {
                                            let is_basic_type = value.is_boolean()
                                                || value.is_number()
                                                || value.is_string();
                                            match t.as_str() {
                                                OOMOL_VAR_DATA if !is_basic_type => false,
                                                OOMOL_BIN_DATA | OOMOL_SECRET_DATA => false,
                                                _ => true,
                                            }
                                        }
                                        _ => true,
                                    })
                            }
                            _ => Some(true),
                        })
                        .unwrap_or(true);

                    if let Some(obj) = value.as_object() {
                        if obj.contains_key("__OOMOL_TYPE__") {
                            cacheable = false;
                        }
                    }

                    block_status.output(
                        job_id,
                        Arc::new(OutputValue { value, cacheable }),
                        handle,
                        done,
                    );
                }
                scheduler::ReceiveMessage::BlockFinished { error, job_id, .. } => {
                    reporter.done(&error);
                    if let Some(error) = error {
                        block_status.done(job_id, Some(error));
                    } else {
                        block_status.done(job_id, None);
                    }
                }
                scheduler::ReceiveMessage::BlockError { error, .. } => {
                    reporter.error(&error);
                }
            }
        }
    })
}
