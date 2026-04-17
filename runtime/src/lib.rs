mod block_job;
pub mod block_status;
pub mod delay_abort;
mod flow_job;
pub mod remote_task_config;
mod run;
pub mod shared;
use mainframe::reporter::ErrorDetail;
use mainframe::scheduler::{BlockRequest, BlockResponseParams, QueryBlockRequest};
use manifest_reader::path_finder::BlockPathFinder;
use std::{
    collections::{HashMap, HashSet},
    env::current_dir,
    path::PathBuf,
    sync::Arc,
};
use tokio::signal::unix::{SignalKind, signal};
use vault::VaultClient;

use tracing::{error as log_error, info, warn};

use job::{BlockJobStacks, JobId, RuntimeScope};
use manifest_meta::{Block, BlockResolver, MergeInputsValue, NodeId, read_flow_or_block};
use utils::error::Result;

use crate::{
    block_job::{TaskJobParameters, execute_task_job},
    flow_job::{
        FlowJobParameters, NodeInputValues, RunBlockSuccessResponse, execute_flow_job,
        get_flow_cache_path, parse_node_downstream, parse_oauth_request, parse_query_block_request,
        parse_run_block_request,
    },
    run::{CommonJobParameters, JobParams, run_job},
};

const SESSION_CANCEL_INFO: &str = "Cancelled";

#[cfg(test)]
pub(crate) static CONNECTOR_ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> =
    std::sync::OnceLock::new();

pub struct RunArgs<'a> {
    pub shared: Arc<shared::Shared>,
    pub block_name: &'a str,
    pub block_reader: BlockResolver,
    pub path_finder: BlockPathFinder,
    pub job_id: Option<JobId>,
    pub nodes: Option<HashSet<String>>,
    pub inputs: Option<String>,
    pub nodes_inputs: Option<String>,
    pub default_package_path: Option<PathBuf>,
    pub project_data: &'a PathBuf,
    pub pkg_data_root: &'a PathBuf,
    pub in_layer: bool,
    pub vault_client: Option<VaultClient>,
}

pub async fn run(args: RunArgs<'_>) -> Result<()> {
    let RunArgs {
        shared,
        block_name,
        block_reader,
        mut path_finder,
        job_id: param_job_id,
        nodes,
        inputs,
        nodes_inputs,
        default_package_path,
        project_data,
        pkg_data_root,
        in_layer,
        vault_client,
    } = args;
    let (block_status_tx, block_status_rx) = block_status::create();
    let root_job_id = param_job_id.unwrap_or_else(JobId::random);
    let stacks = BlockJobStacks::new();
    let partial = nodes.is_some();
    let cache = shared.use_cache;

    let vault_client = Arc::new(vault_client);

    let mut block_reader = block_reader;

    let block = match read_flow_or_block(block_name, &mut block_reader, &mut path_finder) {
        Ok(block) => block,
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            // 解析文件失败时，不会运行任何 block。汇报一次 session 开始结束。
            // 错误信息会输出在 stderr 同时 exit code 会以非零状态输出。
            shared.reporter.session_started(block_name, partial, cache);
            shared.reporter.session_finished(
                block_name,
                &Some(format!("Failed to read block {err:?}")),
                &None,
                partial,
                cache,
            );
            return Err(err);
        }
    };

    let block_path = block.path_str().unwrap_or_else(|| block_name.to_string());

    shared.reporter.session_started(&block_path, partial, cache);

    let nodes = nodes.map(|nodes| nodes.into_iter().map(NodeId::new).collect());

    let scope_workspace = default_package_path
        .filter(|path| path.exists())
        .or_else(|| current_dir().ok());

    let workspace = scope_workspace.ok_or_else(|| {
        utils::error::Error::new(
            "workspace not found: default_package_path does not exist and current_dir is unavailable",
        )
    })?;

    if let Some(patch_value_str) = nodes_inputs {
        let merge_inputs_value = serde_json::from_str::<MergeInputsValue>(&patch_value_str)
            .map_err(|e| {
                log_error!("Failed to parse input values: {}", e);
                format!("Invalid input values: {e}")
            })?;
        if let Block::Flow(flow_block) = &block {
            let mut flow_guard = flow_block.write().unwrap();
            flow_guard.merge_input_values(merge_inputs_value);
        }
    }

    let mut inputs = if let Some(block_values) = inputs {
        serde_json::from_str::<job::BlockInputs>(&block_values)
            .map_err(|e| {
                log_error!("Failed to parse block values: {}", e);
                format!("Invalid block values: {e}")
            })
            .ok()
    } else {
        None
    };

    if let Some(ref inputs) = inputs {
        if let Some(inputs_def) = block.inputs_def() {
            for (handle, input_def) in inputs_def.iter() {
                if inputs.get(handle).is_some() && input_def.is_variable() {
                    warn!(
                        "Input handle {} is a variable but defined as non-variable in block",
                        handle
                    )
                } else if inputs.get(handle).is_none()
                    && input_def.nullable.unwrap_or(false)
                    && input_def.value.is_none()
                {
                    warn!("missing input handle {} in block", handle);
                }
            }
        }
    }

    if let Some(ref inputs_def) = block.inputs_def() {
        let mut pass_through_inputs = inputs.unwrap_or_default();
        for (handle, input_def) in inputs_def.iter() {
            if pass_through_inputs.contains_key(handle) {
                continue;
            }
            if let Some(value) = &input_def.value {
                pass_through_inputs.insert(
                    handle.clone(),
                    Arc::new(utils::output::OutputValue::new(
                        value.clone().unwrap_or_else(|| serde_json::Value::Null),
                        true,
                    )),
                );
            } else if input_def.nullable.unwrap_or(false) {
                pass_through_inputs.insert(
                    handle.clone(),
                    Arc::new(utils::output::OutputValue::new(
                        serde_json::Value::Null,
                        true,
                    )),
                );
            }
        }
        inputs = Some(pass_through_inputs);
    }

    let root_scope = RuntimeScope {
        session_id: shared.session_id.clone(),
        pkg_name: None,
        path: workspace.clone(),
        data_dir: project_data.to_string_lossy().to_string(),
        pkg_root: pkg_data_root.to_path_buf(),
        node_id: None,
        enable_layer: in_layer,
        is_inject: false,
    };

    let common_job_params = CommonJobParameters {
        shared: shared.clone(),
        stacks: stacks.clone(),
        job_id: root_job_id.clone(),
        inputs,
        block_status: block_status_tx.clone(),
        scope: root_scope.clone(),
    };

    let job_params = match block {
        Block::Task(task_block) => JobParams::Task {
            inputs_def: task_block.inputs_def.clone(),
            outputs_def: task_block.outputs_def.clone(),
            task_block: task_block.clone(),
            parent_flow: None,
            timeout: None,
            inputs_def_patch: None,
            common: common_job_params,
        },
        Block::Flow(flow_block) => {
            let flow_cache_path = {
                let flow_guard = flow_block.read().unwrap();
                get_flow_cache_path(&flow_guard.path_str)
            };
            JobParams::Flow {
                flow_block: flow_block.clone(),
                nodes,
                parent_scope: root_scope.clone(),
                node_value_store: match (shared.use_cache, flow_cache_path) {
                    (true, Some(cache_path)) => NodeInputValues::recover_from(cache_path, true),
                    _ => NodeInputValues::new(true),
                },
                slot_blocks: None,
                path_finder: path_finder.clone(),
                common: common_job_params,
                vault_client: vault_client.clone(),
            }
        }
        Block::Service(service_block) => JobParams::Service {
            service_block: service_block.clone(),
            parent_flow: None,
            inputs_def_patch: None,
            common: common_job_params,
        },
        Block::Slot(slot_block) => JobParams::Slot {
            slot_block: slot_block.clone(),
            common: common_job_params,
        },
        Block::Condition(condition_block) => JobParams::Condition {
            condition_block: condition_block.clone(),
            common: common_job_params,
            output_def: None,
        },
    };

    let handle = run_job(job_params);

    let block_status_tx_clone = block_status_tx.clone();
    let signal_handler = tokio::task::spawn(async move {
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = sigint.recv() => {
                log_error!("Received SIGINT");
                block_status_tx_clone.error(SESSION_CANCEL_INFO.to_owned());
            }
            _ = sigterm.recv() => {
                log_error!("Received SIGTERM");
                block_status_tx_clone.error(SESSION_CANCEL_INFO.to_owned());
            }
        }
    });

    let mut result_error: Option<String> = None;
    let mut result_error_detail: Option<ErrorDetail> = None;
    let mut addition_running_jobs = HashSet::new();
    while let Some(status) = block_status_rx.recv().await {
        match status {
            block_status::Status::Outputs { .. } => {}
            block_status::Status::Output { .. } => {}
            block_status::Status::Request(request) => match request {
                BlockRequest::RunBlock(request) => {
                    let res = parse_run_block_request(
                        &request,
                        &mut block_reader,
                        &mut path_finder,
                        shared.clone(),
                        root_scope.clone(),
                    );
                    match res {
                        Ok(response) => match response {
                            RunBlockSuccessResponse::Task {
                                task_block,
                                inputs,
                                inputs_def,
                                outputs_def,
                                scope,
                                request_stack,
                                job_id,
                                node_id,
                            } => {
                                if execute_task_job(TaskJobParameters {
                                    inputs_def,
                                    outputs_def,
                                    dir: block_job::block_dir(&task_block, None, Some(&scope)),
                                    executor: task_block.executor.clone(),
                                    shared: shared.clone(),
                                    stacks: request_stack.stack(
                                        root_job_id.clone(),
                                        block_path.to_owned(),
                                        node_id.clone(),
                                    ),
                                    block_path: task_block.path_str(),
                                    flow_path: None,
                                    injection_store: None,
                                    job_id: job_id.clone(),
                                    inputs: Some(inputs),
                                    block_status: block_status_tx.clone(),
                                    scope,
                                    timeout: None,
                                    inputs_def_patch: None,
                                })
                                .is_some()
                                {
                                    addition_running_jobs.insert(job_id);
                                }
                            }
                            RunBlockSuccessResponse::Flow {
                                flow_block,
                                inputs,
                                scope,
                                request_stack,
                                job_id,
                                node_id,
                            } => {
                                if execute_flow_job(FlowJobParameters {
                                    flow_block,
                                    shared: shared.clone(),
                                    stacks: request_stack.stack(
                                        root_job_id.clone(),
                                        block_path.to_owned(),
                                        node_id.clone(),
                                    ),
                                    vault_client: vault_client.clone(),
                                    flow_job_id: job_id.clone(),
                                    inputs: Some(inputs),
                                    node_value_store: NodeInputValues::new(false),
                                    parent_block_status: block_status_tx.clone(),
                                    nodes: None,
                                    parent_scope: root_scope.clone(),
                                    scope,
                                    slot_blocks: Default::default(),
                                    path_finder: path_finder.clone(),
                                })
                                .is_some()
                                {
                                    addition_running_jobs.insert(job_id);
                                }
                            }
                        },
                        Err(err) => {
                            let msg =
                                format!("Run block failed: {}. Block: {}", err, request.block);
                            tracing::warn!("{}", msg);
                            shared.scheduler_tx.respond_block_request(
                                &shared.session_id,
                                BlockResponseParams {
                                    session_id: shared.session_id.clone(),
                                    job_id: request.job_id.clone(),
                                    error: Some(msg),
                                    result: None,
                                    request_id: request.request_id,
                                },
                            );
                        }
                    }
                }
                BlockRequest::QueryBlock(req) => {
                    let res = parse_query_block_request(&req, &mut block_reader, &mut path_finder);
                    let QueryBlockRequest {
                        session_id,
                        job_id,
                        request_id,
                        ..
                    } = req;
                    match res {
                        Ok(json) => {
                            shared.scheduler_tx.respond_block_request(
                                &session_id,
                                BlockResponseParams {
                                    session_id: session_id.clone(),
                                    job_id: job_id.clone(),
                                    error: None,
                                    result: Some(json),
                                    request_id,
                                },
                            );
                        }
                        Err(err) => {
                            tracing::warn!("{}", err);
                            shared.scheduler_tx.respond_block_request(
                                &session_id,
                                BlockResponseParams {
                                    session_id: session_id.clone(),
                                    job_id: job_id.clone(),
                                    result: None,
                                    error: Some(err),
                                    request_id,
                                },
                            );
                        }
                    }
                }
                BlockRequest::QueryDownstream {
                    request_id, job_id, ..
                } => {
                    let res = parse_node_downstream(None, &HashMap::new(), &None, &None);
                    match res {
                        Ok(json) => {
                            shared.scheduler_tx.respond_block_request(
                                &shared.session_id,
                                BlockResponseParams {
                                    session_id: shared.session_id.clone(),
                                    job_id: job_id.clone(),
                                    error: None,
                                    result: Some(json),
                                    request_id,
                                },
                            );
                        }
                        Err(err) => {
                            tracing::warn!("Query downstream failed: {}.", err,);
                            shared.scheduler_tx.respond_block_request(
                                &shared.session_id,
                                BlockResponseParams {
                                    session_id: shared.session_id.clone(),
                                    job_id: job_id.clone(),
                                    result: None,
                                    error: Some(err),
                                    request_id,
                                },
                            );
                        }
                    }
                }
                BlockRequest::Preview { .. } => {}
                BlockRequest::QueryAuth {
                    payload,
                    request_id,
                    job_id,
                    ..
                } => {
                    if let Some(vault_client) = &*vault_client {
                        let result = parse_oauth_request(&payload, vault_client).await;
                        match result {
                            Ok(res) => {
                                let json = serde_json::to_value(res).unwrap_or_default();
                                shared.scheduler_tx.respond_block_request(
                                    &shared.session_id,
                                    BlockResponseParams {
                                        session_id: shared.session_id.clone(),
                                        job_id: job_id.clone(),
                                        error: None,
                                        result: Some(json),
                                        request_id,
                                    },
                                );
                            }
                            Err(err) => {
                                tracing::warn!("OAuth request failed: {}.", err);
                                shared.scheduler_tx.respond_block_request(
                                    &shared.session_id,
                                    BlockResponseParams {
                                        session_id: shared.session_id.clone(),
                                        job_id: job_id.clone(),
                                        result: None,
                                        error: Some(err.to_string()),
                                        request_id,
                                    },
                                );
                            }
                        }
                    } else {
                        tracing::warn!("OAuth request received but no vault client available.");
                        shared.scheduler_tx.respond_block_request(
                            &shared.session_id,
                            BlockResponseParams {
                                session_id: shared.session_id.clone(),
                                job_id: job_id.clone(),
                                error: Some("Vault client is not available.".to_string()),
                                result: None,
                                request_id,
                            },
                        );
                    }
                }
                BlockRequest::UpdateNodeWeight { .. } => {}
            },
            block_status::Status::Progress { .. } => {}
            block_status::Status::Done {
                error,
                job_id,
                error_detail,
                ..
            } => {
                if job_id != root_job_id && addition_running_jobs.remove(&job_id) {
                    continue;
                }

                if let Some(err) = error {
                    result_error = Some(err);
                    result_error_detail = error_detail;
                    break;
                }
                if addition_running_jobs.is_empty() {
                    break;
                }
            }
            block_status::Status::Error { error } => {
                // it should never happen now
                tracing::warn!("this should never happen: {}", error);
                result_error = Some(error);
                break;
            }
        };
    }

    signal_handler.abort();
    shared.reporter.session_finished(
        &block_path,
        &result_error,
        &result_error_detail,
        partial,
        cache,
    );
    info!(
        "session finished: {}. error: {:?}",
        block_path, result_error
    );

    drop(handle);

    if let Some(err) = result_error {
        return Err(utils::error::Error::new(&err));
    }

    Ok(())
}

pub struct GetPackageArgs<'a> {
    pub block: &'a str,
    pub block_reader: BlockResolver,
    pub path_finder: BlockPathFinder,
    pub nodes: Option<HashSet<String>>,
}

pub fn get_packages(args: GetPackageArgs<'_>) -> Result<HashMap<PathBuf, String>> {
    let GetPackageArgs {
        block,
        mut block_reader,
        mut path_finder,
        nodes,
    } = args;

    // TODO: 支持查询特定 node 需要的 packages
    let filter_nodes = nodes.unwrap_or_default();

    let mut packages: Vec<PathBuf> = vec![];
    match read_flow_or_block(block, &mut block_reader, &mut path_finder) {
        Ok(block) => match block {
            Block::Flow(flow) => {
                let flow_guard = flow.read().unwrap();
                flow_guard
                    .nodes
                    .iter()
                    .filter(|node| {
                        filter_nodes.is_empty() || filter_nodes.contains(&node.0.to_string())
                    })
                    .for_each(|node| {
                        if let Some(package_path) = node.1.package_path() {
                            packages.push(package_path);
                        }
                    });
            }
            _ => {
                return Err("wrong block type. except flow get others".into());
            }
        },
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            return Err(err);
        }
    };

    let mut package_layers = HashMap::new();
    packages.iter().for_each(|package| {
        if manifest_reader::reader::should_skip_package_layer_handling_for_path(package) {
            package_layers.insert(package.clone(), "true".to_owned());
            return;
        }
        let layers = layer::package_layer_status(package);
        if let Ok(layer::PackageLayerStatus::Exist) = layers {
            package_layers.insert(package.clone(), "true".to_owned());
        } else {
            package_layers.insert(package.clone(), "false".to_owned());
        }
    });

    Ok(package_layers)
}

pub struct FindUpstreamArgs<'a> {
    pub block_name: &'a str,
    pub block_reader: BlockResolver,
    pub path_finder: BlockPathFinder,
    pub use_cache: bool,
    pub nodes: Option<HashSet<String>>,
}

pub fn find_upstream(
    args: FindUpstreamArgs<'_>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let FindUpstreamArgs {
        block_name,
        mut block_reader,
        use_cache,
        mut path_finder,
        nodes,
    } = args;

    let block = match read_flow_or_block(block_name, &mut block_reader, &mut path_finder) {
        Ok(block) => block,
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            return Err(err);
        }
    };

    let block_path = block.path_str().unwrap_or_else(|| block_name.to_string());

    match block {
        Block::Flow(flow) => {
            let args = flow_job::UpstreamParameters {
                flow_block: flow,
                use_cache,
                nodes: nodes.map(|nodes| nodes.into_iter().map(NodeId::new).collect()),
            };

            Ok(flow_job::find_upstream(args))
        }
        _ => {
            log_error!("Block is not a flow block: {}", block_path);
            Err("wrong block type. except flow get others".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use flume::{Receiver, Sender};
    use mainframe::{
        MessageData,
        reporter::{self, ReporterRxImpl, ReporterTxImpl},
        scheduler::{self, ExecutorParameters, SchedulerRxImpl, SchedulerTxImpl},
    };
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    struct LoopbackSchedulerTx {
        tx: Sender<MessageData>,
    }

    #[async_trait]
    impl SchedulerTxImpl for LoopbackSchedulerTx {
        async fn send_block_event(&self, _session_id: &job::SessionId, data: MessageData) {
            let _ = self.tx.send_async(data).await;
        }

        async fn send_inputs(&self, _job_id: &job::JobId, _data: MessageData) {}

        async fn run_block(&self, _executor_name: &str, _data: MessageData) {}

        async fn respond_block_request(
            &self,
            _session_id: &job::SessionId,
            _request_id: &str,
            _data: MessageData,
        ) {
        }

        async fn run_service_block(&self, _executor_name: &str, _data: MessageData) {}

        async fn disconnect(&self) {}
    }

    struct LoopbackSchedulerRx {
        rx: Receiver<MessageData>,
    }

    #[async_trait]
    impl SchedulerRxImpl for LoopbackSchedulerRx {
        async fn recv(&mut self) -> MessageData {
            self.rx.recv_async().await.unwrap_or_default()
        }
    }

    struct CollectReporterTx {
        tx: Sender<serde_json::Value>,
    }

    #[async_trait]
    impl ReporterTxImpl for CollectReporterTx {
        async fn send(&self, data: MessageData) {
            if let Ok(message) = serde_json::from_slice::<serde_json::Value>(&data) {
                let _ = self.tx.send_async(message).await;
            }
        }

        async fn disconnect(&self) {}
    }

    struct NoopReporterRx;

    impl ReporterRxImpl for NoopReporterRx {
        fn event_loop(self) -> tokio::task::JoinHandle<()> {
            tokio::spawn(async {})
        }
    }

    struct TestRuntime {
        shared: Arc<shared::Shared>,
        scheduler_handle: tokio::task::JoinHandle<()>,
        reporter_handle: tokio::task::JoinHandle<()>,
        delay_abort_handle: tokio::task::JoinHandle<()>,
        reporter_rx: Receiver<serde_json::Value>,
    }

    impl TestRuntime {
        fn new(project_root: &PathBuf) -> Self {
            let session_id = job::SessionId::random();
            let (scheduler_impl_tx, scheduler_impl_rx) = flume::unbounded();
            let (scheduler_tx, scheduler_rx) = scheduler::create(
                LoopbackSchedulerTx {
                    tx: scheduler_impl_tx,
                },
                LoopbackSchedulerRx {
                    rx: scheduler_impl_rx,
                },
                None,
                None,
                ExecutorParameters {
                    addr: "127.0.0.1:0".to_string(),
                    session_id: session_id.clone(),
                    session_dir: project_root.join(".tmp-session").display().to_string(),
                    pass_through_env_keys: vec![],
                    bind_paths: vec![],
                    env_file: None,
                    tmp_dir: std::env::temp_dir(),
                    debug: false,
                    wait_for_client: false,
                },
                project_root.display().to_string(),
            );

            let (reporter_tx, reporter_rx) = flume::unbounded();
            let (reporter, reporter_loop) = reporter::create(
                session_id.clone(),
                Some(CollectReporterTx { tx: reporter_tx }),
                Some(NoopReporterRx),
            );

            let (delay_abort_tx, delay_abort_rx) = crate::delay_abort::delay_abort();

            Self {
                shared: Arc::new(shared::Shared {
                    session_id,
                    address: "127.0.0.1:0".to_string(),
                    connector_base_url: None,
                    connector_auth_token: None,
                    scheduler_tx,
                    delay_abort_tx,
                    reporter,
                    use_cache: false,
                    remote_task_config: None,
                }),
                scheduler_handle: scheduler_rx.event_loop(),
                reporter_handle: reporter_loop.event_loop(),
                delay_abort_handle: delay_abort_rx.run(),
                reporter_rx,
            }
        }

        async fn shutdown(self) -> Receiver<serde_json::Value> {
            self.shared.scheduler_tx.abort();
            self.shared.reporter.abort();
            let reporter_rx = self.reporter_rx;
            drop(self.shared);
            let _ = self.scheduler_handle.await;
            let _ = self.reporter_handle.await;
            self.delay_abort_handle.abort();
            reporter_rx
        }
    }

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .canonicalize()
            .unwrap()
    }

    fn find_block_finished_result(
        messages: &[serde_json::Value],
        node_id: &str,
    ) -> Option<serde_json::Value> {
        messages.iter().find_map(|message| {
            let message_node_id = message
                .get("stacks")
                .and_then(|stacks| stacks.as_array())
                .and_then(|stacks| stacks.last())
                .and_then(|level| level.get("node_id"))
                .and_then(|node| node.as_str());

            if message.get("type").and_then(|ty| ty.as_str()) == Some("BlockFinished")
                && message_node_id == Some(node_id)
            {
                message.get("result").cloned()
            } else {
                None
            }
        })
    }

    #[test]
    fn get_packages_reports_connector_packages_as_enabled() {
        let root = project_root();
        let flow_path = root.join("tests/fixtures/query-connector-package");
        let search_path = root.join("tests/fixtures");

        let packages = get_packages(GetPackageArgs {
            block: flow_path.to_str().unwrap(),
            block_reader: BlockResolver::new(),
            path_finder: BlockPathFinder::new(root, Some(vec![search_path])),
            nodes: None,
        })
        .expect("query package should succeed");

        let connector_package = project_root()
            .join("tests/fixtures/@connector/demo")
            .canonicalize()
            .unwrap();

        assert_eq!(packages.get(&connector_package), Some(&"true".to_string()));
    }

    #[tokio::test]
    async fn connector_executor_runs_inside_a_flow_chain() {
        let _env_guard = CONNECTOR_ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap();

        let mock_server = MockServer::start().await;
        let previous_oomol_token = std::env::var_os("OOMOL_TOKEN");
        Mock::given(method("POST"))
            .and(path("/v1/actions/echo-output"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "input": {
                    "input": "upstream-payload"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "message": "ok",
                "data": {
                    "forwardedOutput": "connector-ok"
                },
                "meta": {
                    "executionId": "exec-1",
                    "actionId": "echo-output"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/actions/confirm-output"))
            .and(header("authorization", "Bearer test-token"))
            .and(body_json(serde_json::json!({
                "input": {
                    "payload": {
                        "forwardedOutput": "connector-ok"
                    }
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "message": "ok",
                "data": {
                    "confirmed": "connector-ok"
                },
                "meta": {
                    "executionId": "exec-2",
                    "actionId": "confirm-output"
                }
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let root = project_root();
        let runtime = TestRuntime::new(&root);
        let flow_path = root.join("tests/fixtures/connector-flow.oo.yaml");
        let previous_connector_base_url = std::env::var_os("OOCANA_CONNECTOR_BASE_URL");

        unsafe {
            std::env::set_var("OOCANA_CONNECTOR_BASE_URL", mock_server.uri());
            std::env::set_var("OOMOL_TOKEN", "test-token");
        }

        let run_result = run(RunArgs {
            shared: runtime.shared.clone(),
            block_name: flow_path.to_str().unwrap(),
            block_reader: BlockResolver::new(),
            path_finder: BlockPathFinder::new(root.clone(), None),
            job_id: None,
            nodes: None,
            inputs: None,
            nodes_inputs: None,
            default_package_path: None,
            project_data: &root,
            pkg_data_root: &root,
            in_layer: false,
            vault_client: None,
        })
        .await;

        unsafe {
            if let Some(value) = previous_connector_base_url {
                std::env::set_var("OOCANA_CONNECTOR_BASE_URL", value);
            } else {
                std::env::remove_var("OOCANA_CONNECTOR_BASE_URL");
            }
            if let Some(value) = previous_oomol_token {
                std::env::set_var("OOMOL_TOKEN", value);
            } else {
                std::env::remove_var("OOMOL_TOKEN");
            }
        }

        let reporter_rx = runtime.shutdown().await;
        let messages: Vec<_> = reporter_rx.try_iter().collect();

        assert!(run_result.is_ok(), "flow run failed: {run_result:?}");

        let connector_result = find_block_finished_result(&messages, "connector")
            .expect("connector node should finish with outputs");
        assert_eq!(
            connector_result.get("output"),
            Some(&serde_json::json!({
                "forwardedOutput": "connector-ok"
            }))
        );

        let after_connector_result = find_block_finished_result(&messages, "after-connector")
            .expect("downstream connector node should finish with outputs");
        assert_eq!(
            after_connector_result.get("confirmed"),
            Some(&serde_json::json!("connector-ok"))
        );
    }
}
