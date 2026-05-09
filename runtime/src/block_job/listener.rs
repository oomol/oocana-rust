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
    OOMOL_BIN_DATA, OOMOL_SECRET_DATA, OOMOL_TYPE_KEY, OOMOL_VAR_DATA, OutputValue,
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
                    executor_name: executor.name(),
                    job_id: job_id.to_owned(),
                    stacks: stacks.vec(),
                    dir: block_dir.to_owned(),
                    executor,
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
                    tracing::info!(
                        "{executor_name} {identifier:?} ({executor_package:?}) executor is ready. block package: {block_scope:?}"
                    );

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
                    debug!("executor {executor_name} exited with code {code}, reason: {reason:?}");
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

                    debug!(
                        "Executor {executor_name} identifier {identifier:?} for package {package:?} timeout after 5s"
                    );
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
                    warn!(
                        "listener wait timeout 3s. job_id: {msg_job_id}. try to run block again, executor will filter duplicate job_id"
                    );
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
                        block_status.finish(job_id, None, Some(error.clone()), None);
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
                        block_status.finish(job_id, Some(output_map), None, None);
                    } else {
                        reporter.finished(None, None);
                        block_status.finish(job_id, None, None, None);
                    }
                }
                scheduler::ReceiveMessage::BlockError { error, .. } => {
                    reporter.error(&error);
                }
                scheduler::ReceiveMessage::BlockRequest(request) => {
                    // Handle block preview request
                    if let scheduler::BlockRequest::Preview {
                        session_id,
                        job_id,
                        payload,
                        request_id,
                    } = request
                    {
                        reporter.preview(&payload);
                        block_status.run_request(scheduler::BlockRequest::Preview {
                            job_id,
                            payload,
                            request_id,
                            session_id,
                        });
                    } else {
                        block_status.run_request(request);
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use job::{BlockJobStacks, SessionId};
    use mainframe::{
        MessageData,
        reporter::{self, ReporterRxImpl, ReporterTxImpl},
        scheduler::{ExecutorParameters, SchedulerRxImpl, SchedulerTxImpl},
    };
    use tokio::time::{Duration, timeout};

    struct NoopSchedulerTx;

    #[async_trait]
    impl SchedulerTxImpl for NoopSchedulerTx {
        async fn send_block_event(&self, _session_id: &SessionId, _data: MessageData) {}

        async fn send_inputs(&self, _job_id: &JobId, _data: MessageData) {}

        async fn run_block(&self, _executor_name: &str, _data: MessageData) {}

        async fn respond_block_request(
            &self,
            _session_id: &SessionId,
            _request_id: &str,
            _data: MessageData,
        ) {
        }

        async fn run_service_block(&self, _executor_name: &str, _data: MessageData) {}

        async fn disconnect(&self) {}
    }

    struct ChannelSchedulerRx {
        rx: flume::Receiver<MessageData>,
    }

    #[async_trait]
    impl SchedulerRxImpl for ChannelSchedulerRx {
        async fn recv(&mut self) -> MessageData {
            self.rx.recv_async().await.unwrap_or_default()
        }
    }

    struct NoopReporterTx;

    #[async_trait]
    impl ReporterTxImpl for NoopReporterTx {
        async fn send(&self, _data: MessageData) {}

        async fn disconnect(&self) {}
    }

    struct NoopReporterRx;

    impl ReporterRxImpl for NoopReporterRx {
        fn event_loop(self) -> tokio::task::JoinHandle<()> {
            tokio::spawn(async {})
        }
    }

    fn test_scope(session_id: SessionId) -> RuntimeScope {
        let root = std::env::temp_dir();
        RuntimeScope {
            session_id,
            pkg_name: None,
            data_dir: root.display().to_string(),
            pkg_root: root.clone(),
            path: root,
            node_id: None,
            is_inject: false,
            enable_layer: false,
        }
    }

    fn test_executor_payload(session_id: SessionId) -> ExecutorParameters {
        ExecutorParameters {
            addr: "127.0.0.1:0".to_string(),
            session_id,
            session_dir: std::env::temp_dir().display().to_string(),
            pass_through_env_keys: vec![],
            bind_paths: vec![],
            env_file: None,
            tmp_dir: std::env::temp_dir(),
            debug: false,
            wait_for_client: false,
        }
    }

    async fn send_worker_message(
        worker_tx: &flume::Sender<MessageData>,
        message: scheduler::ReceiveMessage,
    ) {
        worker_tx
            .send_async(serde_json::to_vec(&message).unwrap())
            .await
            .unwrap();
    }

    async fn wait_for_listener_subscription(
        worker_tx: &flume::Sender<MessageData>,
        block_status_rx: &crate::block_status::BlockStatusRx,
        session_id: &SessionId,
        job_id: &JobId,
    ) {
        for _ in 0..20 {
            send_worker_message(
                worker_tx,
                scheduler::ReceiveMessage::BlockProgress {
                    session_id: session_id.clone(),
                    job_id: job_id.clone(),
                    progress: 42.0,
                },
            )
            .await;

            match timeout(Duration::from_millis(50), block_status_rx.recv()).await {
                Ok(Some(crate::block_status::Status::Progress {
                    job_id: progress_job_id,
                    progress,
                })) => {
                    assert_eq!(progress_job_id, *job_id);
                    assert_eq!(progress, 42.0);
                    return;
                }
                Ok(Some(_)) => panic!("expected progress status"),
                Ok(None) => panic!("block status channel closed"),
                Err(_) => {}
            }
        }

        panic!("listener did not subscribe to scheduler messages");
    }

    #[tokio::test]
    async fn listener_finishes_block_from_scheduler_block_finished_error() {
        let session_id = SessionId::random();
        let job_id = JobId::random();
        let scope = test_scope(session_id.clone());
        let (worker_tx, worker_rx) = flume::unbounded();
        let (scheduler_tx, scheduler_rx) = scheduler::create(
            NoopSchedulerTx,
            ChannelSchedulerRx { rx: worker_rx },
            None,
            None,
            test_executor_payload(session_id.clone()),
            scope.data_dir.clone(),
        );
        let scheduler_handle = scheduler_rx.event_loop();

        let (reporter_tx, _reporter_rx) =
            reporter::create::<NoopReporterTx, NoopReporterRx>(session_id.clone(), None, None);
        let reporter = Arc::new(reporter_tx.block(job_id.clone(), None, BlockJobStacks::new()));
        let (block_status_tx, block_status_rx) = crate::block_status::create();
        let listener_handle = listen_to_worker(ListenerParameters {
            job_id: job_id.clone(),
            block_path: None,
            stacks: BlockJobStacks::new(),
            scheduler_tx: scheduler_tx.clone(),
            inputs: None,
            outputs_def: None,
            inputs_def: None,
            inputs_def_patch: None,
            block_status: block_status_tx,
            reporter,
            executor: None,
            service: None,
            block_dir: scope.data_dir.clone(),
            scope,
            injection_store: None,
            flow_path: None,
        });

        wait_for_listener_subscription(&worker_tx, &block_status_rx, &session_id, &job_id).await;

        send_worker_message(
            &worker_tx,
            scheduler::ReceiveMessage::BlockFinished {
                session_id: session_id.clone(),
                job_id: job_id.clone(),
                result: None,
                error: Some("Executor python-executor exit with code 1".to_string()),
            },
        )
        .await;

        let status = timeout(Duration::from_secs(1), block_status_rx.recv())
            .await
            .expect("listener should emit block status")
            .expect("block status channel should stay open");

        match status {
            crate::block_status::Status::Done {
                job_id: done_job_id,
                result,
                error,
                error_detail,
            } => {
                assert_eq!(done_job_id, job_id);
                assert!(result.is_none());
                assert_eq!(
                    error.as_deref(),
                    Some("Executor python-executor exit with code 1")
                );
                assert!(error_detail.is_none());
            }
            _ => panic!("expected done error status"),
        }

        scheduler_tx.abort();
        scheduler_handle.await.unwrap();
        listener_handle.await.unwrap();
    }
}
