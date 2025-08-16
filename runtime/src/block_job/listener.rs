use std::{collections::HashMap, sync::Arc};

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use mainframe::{
    reporter::BlockReporterTx,
    scheduler::{self, ExecutorParams, SchedulerTx, ServiceParams},
};
use manifest_meta::{
    HandleName, InjectionStore, InputDefPatchMap, InputHandles, OutputHandles,
    ServiceExecutorOptions, TaskBlockExecutor,
};
use serde_json::Value;
use tracing::{debug, warn};
use utils::output::{
    OutputValue, OOMOL_BIN_DATA, OOMOL_SECRET_DATA, OOMOL_TYPE_KEY, OOMOL_VAR_DATA,
};

use crate::block_status::BlockStatusTx;

#[derive(Debug)]
pub struct ServiceExecutorPayload {
    pub block_name: String,
    pub options: ServiceExecutorOptions,
    pub executor_name: String,
}

pub struct ListenerParameters {
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
    pub executor: Option<Arc<TaskBlockExecutor>>,
    pub service: Option<ServiceExecutorPayload>,
    pub block_dir: String,
    pub scope: RuntimeScope,
    pub injection_store: Option<InjectionStore>,
    pub flow_path: Option<String>,
}

fn is_json_serializable(
    handle: &HandleName,
    value: &Value,
    outputs_def: &Option<OutputHandles>,
) -> bool {
    if let Some(obj) = value.as_object() {
        if obj.contains_key(OOMOL_TYPE_KEY) {
            return false;
        }
    }

    outputs_def
        .as_ref()
        .and_then(|outputs| outputs.get(handle))
        .and_then(|output| output.json_schema.as_ref())
        .and_then(|schema| match schema {
            Value::Object(obj) => obj
                .get("contentMediaType")
                .map(|media_type| match media_type {
                    Value::String(t) => {
                        let is_basic_type =
                            value.is_boolean() || value.is_number() || value.is_string();
                        match t.as_str() {
                            OOMOL_VAR_DATA => is_basic_type,
                            OOMOL_BIN_DATA | OOMOL_SECRET_DATA => false,
                            _ => true,
                        }
                    }
                    _ => true,
                }),
            _ => Some(true),
        })
        .unwrap_or(true)
}

pub fn listen_to_worker(params: ListenerParameters) -> tokio::task::JoinHandle<()> {
    let ListenerParameters {
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
        flow_path,
    } = params;

    let block_scope = scheduler_tx.calculate_scope(&scope);

    let (job_tx, job_rx) = flume::unbounded::<scheduler::ReceiveMessage>();

    scheduler_tx.register_subscriber(job_id.to_owned(), job_tx);
    tokio::spawn(async move {
        let run_block = |executor: Option<&Arc<TaskBlockExecutor>>,
                         service: Option<&ServiceExecutorPayload>| {
            if let Some(executor) = executor {
                scheduler_tx.send_to_executor(ExecutorParams {
                    executor_name: &executor.name(),
                    job_id: job_id.to_owned(),
                    stacks: stacks.vec(),
                    dir: block_dir.to_owned(),
                    executor: &executor,
                    outputs: &outputs_def,
                    scope: &scope,
                    injection_store: &injection_store,
                    flow_path: &flow_path,
                });
            } else if let Some(service) = service {
                scheduler_tx.send_to_service(ServiceParams {
                    executor_name: &service.executor_name,
                    block_name: &service.block_name,
                    job_id: job_id.to_owned(),
                    stacks: stacks.vec(),
                    dir: block_dir.to_owned(),
                    options: &service.options,
                    outputs: &outputs_def,
                    scope: &scope,
                    flow_path: &flow_path,
                });
            }
        };
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
                        debug!(
                            "executor {} identifier {:?} is not equal to block identifier {:?}",
                            executor_name,
                            identifier,
                            block_scope.identifier()
                        );
                        continue;
                    }

                    if let Some(ref executor) = executor {
                        if executor_name != executor.name() {
                            debug!(
                                "executor {} is not equal to block executor {}",
                                executor_name,
                                executor.name()
                            );
                            continue;
                        }
                        run_block(Some(executor), None);
                    } else if let Some(ref service) = service {
                        if executor_name != service.executor_name {
                            debug!(
                                "executor {} is not equal to block executor {}",
                                executor_name, service.executor_name
                            );
                            continue;
                        }
                        run_block(None, Some(service));
                    }
                }
                scheduler::ReceiveMessage::BlockProgress {
                    job_id, progress, ..
                } => {
                    block_status.progress(job_id, progress);
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

                    reporter.finished(None, Some(msg.clone()));
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
                        debug!(
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
                    reporter.finished(None, Some(error_message));
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
                    warn!("listener wait timeout 3s. job_id: {msg_job_id}. try to run block again, executor will filter duplicate job_id");
                    run_block(executor.as_ref(), service.as_ref());
                }
                scheduler::ReceiveMessage::BlockOutputs {
                    job_id, outputs, ..
                } => {
                    let mut reporter_map = HashMap::new();
                    let mut output_map = HashMap::new();
                    for (key, value) in outputs.iter() {
                        output_map.insert(
                            key.clone(),
                            Arc::new(OutputValue {
                                value: value.clone(),
                                is_json_serializable: is_json_serializable(
                                    key,
                                    value,
                                    &outputs_def,
                                ),
                            }),
                        );
                        reporter_map.insert(key.to_string(), value.clone());
                    }
                    block_status.outputs(job_id, output_map);
                    reporter.outputs(&reporter_map);
                }
                scheduler::ReceiveMessage::BlockOutput {
                    output: value,
                    handle,
                    job_id,
                    options,
                    ..
                } => {
                    reporter.output(&value, &handle);

                    let cacheable = is_json_serializable(&handle, &value, &outputs_def);

                    block_status.output(
                        job_id,
                        Arc::new(OutputValue {
                            value,
                            is_json_serializable: cacheable,
                        }),
                        handle,
                        options,
                    );
                }
                scheduler::ReceiveMessage::BlockFinished {
                    result,
                    error,
                    job_id,
                    ..
                } => {
                    if let Some(error) = error {
                        block_status.finish(job_id, None, Some(error.clone()));
                        reporter.finished(None, Some(error));
                        continue;
                    }

                    if let Some(result) = result {
                        let mut reporter_map = HashMap::new();
                        let mut output_map = HashMap::new();
                        for (key, value) in result.iter() {
                            output_map.insert(
                                key.clone(),
                                Arc::new(OutputValue {
                                    value: value.clone(),
                                    is_json_serializable: is_json_serializable(
                                        key,
                                        value,
                                        &outputs_def,
                                    ),
                                }),
                            );
                            reporter_map.insert(key.to_string(), value.clone());
                        }
                        reporter.finished(Some(reporter_map), None);
                        block_status.finish(job_id, Some(output_map), None);
                    } else {
                        reporter.finished(None, None);
                        block_status.finish(job_id, None, None);
                    }
                }
                scheduler::ReceiveMessage::BlockError { error, .. } => {
                    reporter.error(&error);
                }
                scheduler::ReceiveMessage::BlockRequest(request) => {
                    block_status.run_request(request);
                }
            }
        }
    })
}
