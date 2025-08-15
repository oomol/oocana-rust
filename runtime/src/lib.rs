mod block_job;
pub mod block_status;
pub mod delay_abort;
mod flow_job;
mod run;
pub mod shared;
use mainframe::scheduler::{BlockRequest, BlockResponseParams, QueryBlockRequest};
use manifest_reader::path_finder::BlockPathFinder;
use std::{
    collections::{HashMap, HashSet},
    default,
    env::current_dir,
    path::PathBuf,
    sync::Arc,
};
use tokio::signal::unix::{signal, SignalKind};

use tracing::{error as log_error, info, warn};

use job::{BlockJobStacks, JobId, RuntimeScope};
use manifest_meta::{read_flow_or_block, Block, BlockResolver, MergeInputsValue, NodeId};
use utils::error::Result;

use crate::{
    block_job::{execute_task_job, TaskJobParameters},
    flow_job::{
        execute_flow_job, flow::get_flow_cache_path, parse_node_downstream,
        parse_query_block_request, parse_run_block_request, FlowJobParameters, NodeInputValues,
        RunBlockSuccessResponse,
    },
    run::{run_job, CommonJobParameters, JobParams},
};

const SESSION_CANCEL_INFO: &str = "Cancelled";

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
    } = args;
    let (block_status_tx, block_status_rx) = block_status::create();
    let root_job_id = param_job_id.unwrap_or_else(JobId::random);
    let stacks = BlockJobStacks::new();
    let partial = nodes.is_some();
    let cache = shared.use_cache;

    let mut block_reader = block_reader;

    let mut block = match read_flow_or_block(block_name, &mut block_reader, &mut path_finder) {
        Ok(block) => block,
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            // 解析文件失败时，不会运行任何 block。汇报一次 session 开始结束。
            // 错误信息会输出在 stderr 同时 exit code 会以非零状态输出。
            shared.reporter.session_started(block_name, partial, cache);
            shared.reporter.session_finished(
                block_name,
                &Some(format!("Failed to read block {:?}", err)),
                partial,
                cache,
            );
            return Err(err);
        }
    };

    let block_path = block
        .path_str()
        .map(|p| p.to_owned())
        .unwrap_or_else(|| block_name.to_string());

    shared.reporter.session_started(&block_path, partial, cache);

    let nodes = nodes.map(|nodes| nodes.into_iter().map(NodeId::new).collect());

    let scope_workspace = default_package_path
        .filter(|path| path.exists())
        .or_else(|| current_dir().ok());

    let workspace = scope_workspace.expect("workspace not found");

    if let Some(patch_value_str) = nodes_inputs {
        let merge_inputs_value = serde_json::from_str::<MergeInputsValue>(&patch_value_str)
            .map_err(|e| {
                log_error!("Failed to parse input values: {}", e);
                format!("Invalid input values: {}", e)
            })?;
        if let Block::Flow(flow_block) = block {
            let mut inner_flow_block = (*flow_block).clone();
            inner_flow_block.merge_input_values(merge_inputs_value);
            block = Block::Flow(Arc::new(inner_flow_block));
        }
    }

    let mut inputs = if let Some(block_values) = inputs {
        serde_json::from_str::<job::BlockInputs>(&block_values)
            .map_err(|e| {
                log_error!("Failed to parse block values: {}", e);
                format!("Invalid block values: {}", e)
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
            let flow_cache_path = get_flow_cache_path(&flow_block.path_str);
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
                                if let Some(_) = execute_task_job(TaskJobParameters {
                                    inputs_def,
                                    outputs_def,
                                    task_block,
                                    shared: shared.clone(),
                                    parent_flow: None,
                                    stacks: request_stack.stack(
                                        root_job_id.clone(),
                                        block_path.to_owned(),
                                        node_id.clone(),
                                    ),
                                    job_id: job_id.clone(),
                                    inputs: Some(inputs),
                                    block_status: block_status_tx.clone(),
                                    scope,
                                    timeout: None,
                                    inputs_def_patch: None,
                                }) {
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
                                if let Some(_) = execute_flow_job(FlowJobParameters {
                                    flow_block,
                                    shared: shared.clone(),
                                    stacks: request_stack.stack(
                                        root_job_id.clone(),
                                        block_path.to_owned(),
                                        node_id.clone(),
                                    ),
                                    flow_job_id: job_id.clone(),
                                    inputs: Some(inputs),
                                    node_value_store: NodeInputValues::new(false),
                                    parent_block_status: block_status_tx.clone(),
                                    nodes: None,
                                    parent_scope: root_scope.clone(),
                                    scope,
                                    slot_blocks: default::Default::default(),
                                    path_finder: path_finder.clone(),
                                }) {
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
            },
            block_status::Status::Progress { .. } => {}
            block_status::Status::Done { error, job_id, .. } => {
                if job_id != root_job_id {
                    if addition_running_jobs.remove(&job_id) {
                        continue;
                    }
                }

                if let Some(err) = error {
                    result_error = Some(err);
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
    shared
        .reporter
        .session_finished(&block_path, &result_error, partial, cache);
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

    let mut packages = vec![];
    match read_flow_or_block(block, &mut block_reader, &mut path_finder) {
        Ok(block) => match block {
            Block::Flow(flow) => {
                flow.nodes
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

    let block_path = block
        .path_str()
        .map(|p| p.to_owned())
        .unwrap_or_else(|| block_name.to_string());

    match block {
        Block::Flow(flow) => {
            let args = flow_job::UpstreamArgs {
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
