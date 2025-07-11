use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};
use uuid::Uuid;

use crate::{
    block_job::{run_block, run_task_block, BlockJobHandle, RunBlockArgs, RunTaskBlockArgs},
    block_status::{self, BlockStatusTx},
    shared::Shared,
};
use mainframe::{
    reporter::FlowReporterTx,
    scheduler::{self, BlockRequest, BlockResponseParams},
};
use tracing::warn;
use utils::output::OutputValue;

use job::{BlockInputs, BlockJobStacks, JobId, RunningPackageScope};
use manifest_meta::{
    BlockResolver, HandleTo, InputHandle, InputHandles, Node, NodeId, OutputHandles, RunningScope,
    Slot, SubflowBlock,
};

use super::node_input_values;
use node_input_values::{CacheMetaMap, CacheMetaMapExt, NodeInputValues};

use super::run_to_node::RunToNode;
pub struct FlowJobHandle {
    // TODO: Remove this field
    #[allow(dead_code)]
    pub job_id: JobId,
    spawn_handle: tokio::task::JoinHandle<()>,
}

impl Drop for FlowJobHandle {
    fn drop(&mut self) {
        self.spawn_handle.abort();
    }
}

struct BlockInFlowJobHandle {
    node_id: NodeId,
    _job: BlockJobHandle,
}

struct FlowShared {
    job_id: JobId,
    flow_block: Arc<SubflowBlock>,
    shared: Arc<Shared>,
    stacks: BlockJobStacks,
    parent_scope: RunningPackageScope,
    scope: RunningPackageScope,
    slot_blocks: HashMap<NodeId, Slot>,
    path_finder: manifest_reader::path_finder::BlockPathFinder,
}

struct RunFlowContext {
    node_input_values: NodeInputValues,
    parent_block_status: BlockStatusTx,
    jobs: HashMap<JobId, BlockInFlowJobHandle>,
    block_status: BlockStatusTx,
    node_queue_pool: HashMap<NodeId, NodeQueue>,
}

#[derive(Default)]
struct NodeQueue {
    pub jobs: HashSet<JobId>,
    pub pending: HashSet<JobId>, // JobId 本身不会使用，只是用来判断 pending 的数量
}

pub struct RunFlowArgs {
    pub flow_block: Arc<SubflowBlock>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub flow_job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub node_value_store: NodeInputValues,
    pub parent_block_status: BlockStatusTx,
    pub nodes: Option<HashSet<NodeId>>,
    pub parent_scope: RunningPackageScope,
    pub scope: RunningPackageScope,
    pub slot_blocks: HashMap<NodeId, Slot>,
    pub path_finder: manifest_reader::path_finder::BlockPathFinder,
}

pub struct UpstreamArgs {
    pub flow_block: Arc<SubflowBlock>,
    pub use_cache: bool,
    pub nodes: Option<HashSet<NodeId>>,
}

pub fn find_upstream(upstream_args: UpstreamArgs) -> (Vec<String>, Vec<String>, Vec<String>) {
    let UpstreamArgs {
        flow_block,
        use_cache,
        nodes,
    } = upstream_args;

    let mut node_input_values = if use_cache & get_flow_cache_path(&flow_block.path_str).is_some() {
        NodeInputValues::recover_from(get_flow_cache_path(&flow_block.path_str).unwrap(), false)
    } else {
        NodeInputValues::new(false)
    };

    let (node_will_run, waiting_nodes, upstream_nodes) = find_upstream_nodes(
        &nodes.unwrap_or_default(),
        &flow_block,
        &mut node_input_values,
    );
    (node_will_run, waiting_nodes, upstream_nodes)
}

pub fn run_flow(mut flow_args: RunFlowArgs) -> Option<BlockJobHandle> {
    let RunFlowArgs {
        flow_block,
        shared,
        stacks,
        flow_job_id,
        inputs,
        parent_block_status,
        node_value_store,
        ref mut nodes,
        slot_blocks,
        scope,
        parent_scope,
        path_finder,
    } = flow_args;

    let reporter = Arc::new(shared.reporter.flow(
        flow_job_id.to_owned(),
        Some(flow_block.path_str.clone()),
        stacks.clone(),
    ));
    reporter.started(&inputs);
    let absence_inputs = flow_block
        .query_inputs()
        .into_iter()
        .filter_map(|(node_id, handles)| {
            if flow_block
                .nodes
                .get(&node_id)
                .is_none_or(|n| node_value_store.is_node_fulfill(&n))
            {
                None
            } else {
                Some((
                    node_id,
                    handles
                        .iter()
                        .filter(|h| h.value.is_none())
                        .cloned()
                        .collect::<Vec<InputHandle>>(),
                ))
            }
        })
        .collect::<HashMap<NodeId, Vec<InputHandle>>>();

    if !absence_inputs.is_empty() {
        let node_and_handles = absence_inputs
            .iter()
            .map(|(node_id, handles)| {
                let handles_name = handles
                    .iter()
                    .map(|input| input.handle.to_string())
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("node({}) handles: [{}]", node_id, handles_name)
            })
            .collect::<Vec<String>>()
            .join(", ");
        // these message is tested in tests/test_run.rs
        warn!(
            "these node won't run because some inputs are not provided: {}",
            node_and_handles,
        );
    }

    let (block_status_tx, block_status_rx) = block_status::create();

    let mut filtered_nodes = nodes.clone();
    let mut run_flow_ctx = RunFlowContext {
        node_input_values: node_value_store,
        parent_block_status,
        jobs: HashMap::new(),
        block_status: block_status_tx,
        node_queue_pool: HashMap::new(),
    };

    let flow_path = flow_block.path.clone();
    let mut flow_shared = FlowShared {
        flow_block,
        job_id: flow_job_id.to_owned(),
        shared,
        stacks,
        scope,
        slot_blocks,
        parent_scope,
        path_finder: path_finder.subflow(flow_path),
    };

    if let Some(ref origin_nodes) = nodes {
        let (runnable_nodes, pending_nodes, upstream_nodes) = find_upstream_nodes(
            origin_nodes,
            &flow_shared.flow_block,
            &mut run_flow_ctx.node_input_values,
        );

        reporter.will_run_nodes(
            &runnable_nodes,
            &pending_nodes,
            &origin_nodes
                .iter()
                .map(|node| node.to_string())
                .collect::<Vec<String>>(),
        );

        for node in runnable_nodes {
            if let Some(node) = flow_shared.flow_block.nodes.get(&NodeId::from(node)) {
                run_node(node, &flow_shared, &mut run_flow_ctx);
            }
        }

        // add upstream nodes to filtered_nodes
        for node in upstream_nodes {
            filtered_nodes
                .as_mut()
                .map(|nodes| nodes.insert(NodeId::from(node)));
        }
    } else {
        let mut runnable_nodes: Vec<String> = Vec::new();
        let mut pending_nodes: Vec<String> = Vec::new();
        for node in flow_shared.flow_block.nodes.values() {
            if run_flow_ctx.node_input_values.is_node_fulfill(node) {
                runnable_nodes.push(node.node_id().to_string());
            } else {
                pending_nodes.push(node.node_id().to_string());
            }
        }

        // 直接把可直接运行之外的节点，都当做中间节点（可以考虑把没有 output 连线的节点当做终点）
        // 目前 UI 只会区分可直接运行的节点，和其他节点（mid 和 end）
        reporter.will_run_nodes(&runnable_nodes, &pending_nodes, &Vec::new());

        for node in runnable_nodes {
            if let Some(node) = flow_shared.flow_block.nodes.get(&NodeId::from(node)) {
                run_node(node, &flow_shared, &mut run_flow_ctx);
            }
        }
    }

    if let Some(inputs) = inputs {
        for (handle, value) in inputs {
            if let Some(handle_tos) = flow_shared.flow_block.flow_inputs_tos.get(&handle) {
                produce_new_value(
                    &value,
                    handle_tos,
                    &flow_shared,
                    &mut run_flow_ctx,
                    true,
                    &filtered_nodes,
                    &reporter,
                );
            }
        }
    }

    if is_finish(&run_flow_ctx) {
        flow_success(&flow_shared, &run_flow_ctx, &reporter);
        return None;
    }

    let scheduler_tx = flow_shared.shared.scheduler_tx.clone();
    let spawn_handle = tokio::spawn(async move {
        while let Some(status) = block_status_rx.recv().await {
            match status {
                block_status::Status::Output {
                    job_id,
                    result,
                    handle,
                } => {
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        if let Some(node) = flow_shared.flow_block.nodes.get(&job.node_id) {
                            if let Some(tos) = node.to() {
                                if let Some(handle_tos) = tos.get(&handle) {
                                    produce_new_value(
                                        &result,
                                        handle_tos,
                                        &flow_shared,
                                        &mut run_flow_ctx,
                                        true,
                                        &filtered_nodes,
                                        &reporter,
                                    );
                                }
                            }
                        }
                    }
                }
                block_status::Status::Outputs {
                    job_id,
                    outputs: map,
                } => {
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        if let Some(node) = flow_shared.flow_block.nodes.get(&job.node_id) {
                            if let Some(tos) = node.to() {
                                for (handle, value) in map.iter() {
                                    if let Some(handle_tos) = tos.get(handle) {
                                        produce_new_value(
                                            value,
                                            handle_tos,
                                            &flow_shared,
                                            &mut run_flow_ctx,
                                            true,
                                            &filtered_nodes,
                                            &reporter,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                block_status::Status::Request(request) => match request {
                    BlockRequest::RunBlock {
                        block,
                        job_id,
                        block_job_id: new_job_id,
                        inputs,
                        request_id,
                        ..
                    } => {
                        let block_path = match flow_shared.path_finder.find_task_block_path(&block)
                        {
                            Ok(path) => path,
                            Err(e) => {
                                let msg = format!(
                                    "Failed to find task block path for block: {}. Error: {}",
                                    block, e
                                );
                                tracing::warn!("{}", msg);
                                scheduler_tx.respond_block_request(
                                    &flow_shared.shared.session_id,
                                    scheduler::BlockResponseParams {
                                        session_id: flow_shared.shared.session_id.clone(),
                                        job_id: job_id.clone().into(),
                                        error: Some(msg),
                                        result: None,
                                        request_id,
                                    },
                                );
                                continue;
                            }
                        };

                        let task_block = match BlockResolver::new().read_task_block(&block_path) {
                            Ok(tb) => tb,
                            Err(e) => {
                                let msg = format!(
                                    "Failed to read task block from path: {}. Error: {}",
                                    block_path.display(),
                                    e
                                );
                                tracing::warn!("{}", msg);
                                scheduler_tx.respond_block_request(
                                    &flow_shared.shared.session_id,
                                    scheduler::BlockResponseParams {
                                        session_id: flow_shared.shared.session_id.clone(),
                                        job_id: job_id.clone().into(),
                                        error: Some(msg),
                                        result: None,
                                        request_id,
                                    },
                                );
                                continue;
                            }
                        };

                        // 构造输入映射
                        let inputs_map: HashMap<_, _> = inputs
                            .into_iter()
                            .map(|(handle, value)| {
                                (
                                    handle,
                                    Arc::new(OutputValue {
                                        value,
                                        cacheable: true,
                                    }),
                                )
                            })
                            .collect();

                        let missing_inputs = task_block
                            .inputs_def
                            .as_ref()
                            .map(|inputs_def| {
                                inputs_def
                                    .iter()
                                    .filter_map(|(handle, _)| {
                                        (!inputs_map.contains_key(handle)).then_some(handle.clone())
                                    })
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();

                        if !missing_inputs.is_empty() {
                            let msg = format!(
                                "Task block {} inputs missing these input handles: {:?}",
                                block, missing_inputs
                            );
                            tracing::warn!("{}", msg);
                            scheduler_tx.respond_block_request(
                                &flow_shared.shared.session_id,
                                scheduler::BlockResponseParams {
                                    session_id: flow_shared.shared.session_id.clone(),
                                    job_id: job_id.clone().into(),
                                    error: Some(msg),
                                    result: None,
                                    request_id,
                                },
                            );
                            continue;
                        }

                        tracing::info!("running task block: {} as {}", block, new_job_id);

                        if let Some(handle) = run_task_block(RunTaskBlockArgs {
                            task_block,
                            shared: Arc::clone(&flow_shared.shared),
                            parent_flow: Some(flow_shared.flow_block.clone()),
                            stacks: flow_shared.stacks.stack(
                                flow_shared.job_id.to_owned(),
                                flow_shared.flow_block.path_str.to_owned(),
                                NodeId::from(format!("run_block::{}", block)),
                            ),
                            job_id: new_job_id.clone().into(),
                            inputs: Some(inputs_map),
                            block_status: run_flow_ctx.block_status.clone(),
                            scope: flow_shared.scope.clone(),
                            timeout: None,
                            inputs_def_patch: None,
                        }) {
                            run_flow_ctx.jobs.insert(
                                new_job_id.into(),
                                BlockInFlowJobHandle {
                                    node_id: NodeId::from(format!("run_block::{}", block)),
                                    _job: handle,
                                },
                            );
                        }
                    }
                    BlockRequest::QueryBlock {
                        session_id,
                        job_id,
                        block,
                        request_id,
                    } => {
                        let block_path = match flow_shared.path_finder.find_task_block_path(&block)
                        {
                            Ok(path) => path,
                            Err(e) => {
                                let msg = format!(
                                    "Failed to find task block path for block: {}. Error: {}",
                                    block, e
                                );
                                tracing::warn!("{}", msg);
                                scheduler_tx.respond_block_request(
                                    &session_id,
                                    scheduler::BlockResponseParams {
                                        session_id: session_id.clone(),
                                        job_id: job_id.clone().into(),
                                        error: Some(msg),
                                        result: None,
                                        request_id,
                                    },
                                );
                                continue;
                            }
                        };

                        let task_block = match BlockResolver::new().read_task_block(&block_path) {
                            Ok(tb) => tb,
                            Err(e) => {
                                let msg = format!(
                                    "Failed to read task block from path: {}. Error: {}",
                                    block_path.display(),
                                    e
                                );
                                tracing::warn!("{}", msg);
                                scheduler_tx.respond_block_request(
                                    &session_id,
                                    scheduler::BlockResponseParams {
                                        session_id: session_id.clone(),
                                        job_id: job_id.clone().into(),
                                        error: Some(msg),
                                        result: None,
                                        request_id,
                                    },
                                );
                                continue;
                            }
                        };

                        #[derive(serde::Serialize)]
                        struct TaskBlockMetadata {
                            pub description: Option<String>,
                            pub inputs_def: Option<InputHandles>,
                            pub outputs_def: Option<OutputHandles>,
                            pub additional_inputs: bool,
                            pub additional_outputs: bool,
                        }

                        let metadata = TaskBlockMetadata {
                            description: task_block.description.clone(),
                            inputs_def: task_block.inputs_def.clone(),
                            outputs_def: task_block.outputs_def.clone(),
                            additional_inputs: task_block.additional_inputs,
                            additional_outputs: task_block.additional_outputs,
                        };
                        let json = serde_json::to_value(&metadata);
                        if let Ok(json) = json {
                            // tracing::debug!("Task block metadata serialized to JSON: {}", json);
                            scheduler_tx.respond_block_request(
                                &session_id,
                                BlockResponseParams {
                                    session_id: session_id.clone(),
                                    job_id: job_id.clone().into(),
                                    error: None,
                                    result: Some(json),
                                    request_id,
                                },
                            );
                        } else {
                            tracing::warn!("Failed to serialize task block metadata to JSON");
                            scheduler_tx.respond_block_request(
                                &session_id,
                                BlockResponseParams {
                                    session_id: session_id.clone(),
                                    job_id: job_id.clone().into(),
                                    result: None,
                                    error: Some(
                                        "Failed to serialize task block metadata to JSON".into(),
                                    ),
                                    request_id,
                                },
                            );
                        }
                    }
                },
                block_status::Status::Done {
                    job_id,
                    result,
                    error,
                } => {
                    run_pending_node(job_id.to_owned(), &flow_shared, &mut run_flow_ctx);

                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        if let Some(node) = flow_shared.flow_block.nodes.get(&job.node_id) {
                            if let Some(tos) = node.to() {
                                for (handle, value) in result.unwrap_or_default().iter() {
                                    if let Some(handle_tos) = tos.get(handle) {
                                        produce_new_value(
                                            value,
                                            handle_tos,
                                            &flow_shared,
                                            &mut run_flow_ctx,
                                            true,
                                            &filtered_nodes,
                                            &reporter,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    if let Some(ref err) = error {
                        save_flow_cache(
                            &run_flow_ctx.node_input_values,
                            &flow_shared.flow_block.path_str,
                        );

                        let node_id = run_flow_ctx
                            .jobs
                            .get(&job_id)
                            .map(|job| job.node_id.to_owned());

                        let is_context_run_block = node_id
                            .as_ref()
                            // TODO: use constants for run_block prefix
                            .map(|id| id.starts_with("run_block::"))
                            .unwrap_or(true);

                        if is_context_run_block {
                            // if the error is from a run_block, we just log the error and continue run flow. It's user's responsibility to handle the error and consider whether to continue the flow.
                            continue;
                        }

                        run_flow_ctx.jobs.clear();

                        if flow_shared.stacks.is_root() {
                            // root already show error one top level message
                            run_flow_ctx.parent_block_status.finish(
                                flow_shared.job_id.to_owned(),
                                None,
                                Some(format!(
                                    "{} failed",
                                    node_id
                                        .map(|n| format!("node id: {n}"))
                                        .unwrap_or_else(|| format!("job_id: {}", job_id))
                                )),
                            );
                        } else {
                            // add error node stack
                            let error_message = format!(
                                "{} failed:\n{}",
                                node_id
                                    .map(|n| format!("node id: {n}"))
                                    .unwrap_or_else(|| format!("job_id: {}", job_id)),
                                err
                            );
                            reporter.done(&Some(error_message.clone()));

                            run_flow_ctx.parent_block_status.finish(
                                flow_shared.job_id.to_owned(),
                                None,
                                Some(error_message),
                            );
                        }
                        break;
                    } else if remove_job_and_is_finished(&job_id, &mut run_flow_ctx) {
                        flow_success(&flow_shared, &run_flow_ctx, &reporter);
                        break;
                    }
                }
                block_status::Status::Error { error } => {
                    save_flow_cache(
                        &run_flow_ctx.node_input_values,
                        &flow_shared.flow_block.path_str,
                    );

                    run_flow_ctx.jobs.clear();
                    run_flow_ctx.parent_block_status.error(error);
                    break;
                }
            };
        }
    });

    Some(BlockJobHandle::new(
        flow_job_id.to_owned(),
        FlowJobHandle {
            job_id: flow_job_id,
            spawn_handle,
        },
    ))
}

fn remove_job_and_is_finished(job_id: &JobId, run_flow_ctx: &mut RunFlowContext) -> bool {
    run_flow_ctx.jobs.remove(job_id);
    is_finish(run_flow_ctx)
}

fn flow_success(shared: &FlowShared, ctx: &RunFlowContext, reporter: &FlowReporterTx) {
    reporter.done(&None);
    ctx.parent_block_status
        .finish(shared.job_id.to_owned(), None, None);
    save_flow_cache(&ctx.node_input_values, &shared.flow_block.path_str);
}

fn run_pending_node(job_id: JobId, flow_shared: &FlowShared, run_flow_ctx: &mut RunFlowContext) {
    if let Some(job_handle) = run_flow_ctx.jobs.get(&job_id) {
        let node_id = job_handle.node_id.to_owned();

        let node_queue = run_flow_ctx
            .node_queue_pool
            .entry(node_id.to_owned())
            .or_default();

        node_queue.jobs.remove(&job_id);

        if let Some(node) = flow_shared.flow_block.nodes.get(&node_id) {
            let concurrency = node.concurrency();
            let jobs_count = node_queue.jobs.len();
            if jobs_count < concurrency as usize && !node_queue.pending.is_empty() {
                if let Some(job_id) = node_queue.pending.iter().next().cloned() {
                    node_queue.pending.remove(&job_id);
                    run_node(node, flow_shared, run_flow_ctx);
                }
            }
        }
    }
}

/// 第一个是可以直接 run 的节点(会包含部分可以直接跑的 origin_nodes）
/// 第二个是等待的节点 nodes（不包含 origin_nodes）
/// 第三个是所有的上游 nodes（不包含 origin_nodes）
fn find_upstream_nodes(
    origin_nodes: &HashSet<NodeId>,
    flow_block: &SubflowBlock,
    node_input_values: &mut NodeInputValues,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let (node_not_found, out_of_side_nodes, node_can_run_directly) =
        calc_nodes(origin_nodes, flow_block, node_input_values);
    // 两部分：
    // 1. nodes 中可以直接 run 的
    // 2. nodes 中的依赖节点中可以直接 run 的节点
    let mut nodes_will_run = node_can_run_directly
        .iter()
        .map(|node| node.node_id().to_owned().into())
        .collect::<Vec<String>>();

    if !node_not_found.is_empty() {
        let not_found_message = node_not_found
            .iter()
            .map(|node_id| node_id.to_string())
            .collect::<Vec<String>>()
            .join(",");
        warn!("some nodes are not found: {}", not_found_message);
    }

    let mut waiting_nodes = Vec::new();
    let mut upstream_nodes = Vec::new();
    for node_id in out_of_side_nodes {
        if let Some(node) = flow_block.nodes.get(&node_id) {
            upstream_nodes.push(node.node_id().to_owned().into());
            if node_input_values.is_node_fulfill(node) {
                nodes_will_run.push(node.node_id().to_owned().into()); // will run outside node
            } else {
                waiting_nodes.push(node.node_id().to_string());
            }
        }
    }

    (nodes_will_run, waiting_nodes, upstream_nodes)
}

fn calc_nodes<'a>(
    nodes: &HashSet<NodeId>,
    flow_block: &'a SubflowBlock,
    node_input_values: &mut NodeInputValues,
) -> (HashSet<NodeId>, HashSet<NodeId>, Vec<&'a Node>) {
    let mut node_id_not_found = HashSet::new();
    let mut dep_node_id_outside_list = HashSet::new();
    let mut node_will_run = Vec::new();

    for node_id in nodes {
        if let Some(node) = flow_block.nodes.get(node_id) {
            let n = RunToNode::new(
                flow_block,
                Some(node_id.to_owned()),
                Some(node_input_values),
            );

            let nodes_without_self = nodes
                .iter()
                .filter(|id| *id != node_id)
                .map(|id| id.to_owned())
                .collect();

            let is_fulfill = node_input_values.is_node_fulfill(node);

            // 如果有依赖的节点在 nodes 列表中，删除在 input value 中的数据，让列表中的依赖节点，来触发该 node 的运行
            if n.has_deps_in(&nodes_without_self) {
                let dep_in_nodes: HashSet<NodeId> = n
                    .find_intersection_nodes(&nodes_without_self)
                    .iter()
                    .map(|id| id.to_owned())
                    .collect();

                node_input_values.remove_input_values(node, &dep_in_nodes);
            }

            if node_input_values.is_node_fulfill(node) {
                node_will_run.push(node);
            } else if !is_fulfill && n.should_run_nodes.is_some() {
                // TODO: 性能优化
                n.should_run_nodes.unwrap().iter().for_each(|node_id| {
                    if !nodes.contains(node_id) {
                        dep_node_id_outside_list.insert(node_id.to_owned());
                    }
                });
            }
        } else {
            node_id_not_found.insert(node_id.to_owned());
        }
    }

    (node_id_not_found, dep_node_id_outside_list, node_will_run)
}

fn produce_new_value(
    value: &Arc<OutputValue>,
    handle_tos: &Vec<HandleTo>,
    shared: &FlowShared,
    ctx: &mut RunFlowContext,
    run_next_node: bool,
    filter_nodes: &Option<HashSet<NodeId>>,
    reporter: &FlowReporterTx,
) {
    for handle_to in handle_tos {
        match handle_to {
            HandleTo::ToNodeInput {
                node_id,
                node_input_handle,
            } => {
                let in_run_nodes = filter_nodes.as_ref().is_some()
                    && filter_nodes
                        .as_ref()
                        .is_some_and(|nodes| nodes.contains(node_id));
                let should_run_output_node =
                    run_next_node && (filter_nodes.as_ref().is_none() || in_run_nodes);

                let previous_pending_fulfill =
                    if let Some(node) = shared.flow_block.nodes.get(node_id) {
                        ctx.node_input_values.node_pending_fulfill(node)
                    } else {
                        0
                    };
                // still need to insert value, even if the node is not in the run_nodes list
                ctx.node_input_values.insert(
                    node_id.to_owned(),
                    node_input_handle.to_owned(),
                    Arc::clone(value),
                );

                if should_run_output_node {
                    if let Some(node) = shared.flow_block.nodes.get(node_id) {
                        if ctx.node_input_values.is_node_fulfill(node) {
                            let node_queue =
                                ctx.node_queue_pool.entry(node_id.to_owned()).or_default();
                            if node_queue.jobs.len() < node.concurrency() as usize {
                                run_node(node, shared, ctx);
                            } else {
                                let pending_fulfill =
                                    ctx.node_input_values.node_pending_fulfill(node);
                                // this value is fulfill the node's input again, we need added a pending job to queue.
                                if pending_fulfill > previous_pending_fulfill {
                                    node_queue.pending.insert(JobId::random());
                                    tracing::info!("node queue ({}) is full, add a pending job. current jobs count: {}, concurrency: {}",node_id,node_queue.jobs.len(),node.concurrency());
                                } else {
                                    tracing::info!(
                                        "this node ({}) is fulfill because has pending job, this input value event not won't trigger because it not fulfill more than before",
                                        node_id
                                    );
                                }
                            }
                        }
                    } else {
                        warn!("node {} not found in flow block", node_id);
                    }
                }
            }
            HandleTo::ToFlowOutput {
                output_handle: flow_output_handle,
            } => {
                reporter.output(value.clone(), flow_output_handle);
                ctx.parent_block_status.output(
                    shared.job_id.to_owned(),
                    Arc::clone(value),
                    flow_output_handle.to_owned(),
                );
            }
        }
    }
}

fn run_node(node: &Node, shared: &FlowShared, ctx: &mut RunFlowContext) {
    let job_id = JobId::random();
    ctx.node_queue_pool
        .entry(node.node_id().to_owned())
        .or_default()
        .jobs
        .insert(job_id.to_owned());

    let block = if matches!(node, Node::Slot(_)) {
        let node_id = node.node_id();
        shared
            .slot_blocks
            .get(node_id)
            .map(|slot| slot.block())
            .unwrap_or_else(|| node.block())
    } else {
        node.block()
    };

    let scope = if matches!(node, Node::Slot(_)) {
        shared
            .slot_blocks
            .get(node.node_id())
            .map(|slot| slot.scope().clone())
            .unwrap_or_else(|| node.scope())
    } else {
        node.scope()
    };

    let package_scope = match scope {
        RunningScope::Package { path, node_id, .. } => RunningPackageScope {
            package_path: path.clone(),
            node_id: node_id.clone(),
            enable_layer: layer::feature_enabled(),
            is_inject: node.scope().is_inject(),
        },
        RunningScope::Flow { node_id, .. } => RunningPackageScope {
            package_path: shared.scope.package_path().to_owned(),
            node_id: node_id.clone(),
            enable_layer: shared.scope.need_layer(),
            is_inject: node.scope().is_inject(),
        },
        RunningScope::Slot { .. } => RunningPackageScope {
            package_path: shared.parent_scope.package_path().to_owned(),
            node_id: None,
            enable_layer: shared.parent_scope.need_layer(),
            is_inject: node.scope().is_inject(),
        },
    };

    let handle = run_block({
        RunBlockArgs {
            block,
            shared: Arc::clone(&shared.shared),
            parent_flow: Some(Arc::clone(&shared.flow_block)),
            stacks: shared.stacks.stack(
                shared.job_id.to_owned(),
                shared.flow_block.path_str.to_owned(),
                node.node_id().to_owned(),
            ),
            node_value_store: None,
            job_id: job_id.to_owned(),
            inputs: ctx.node_input_values.take(node),
            block_status: ctx.block_status.clone(),
            nodes: None,
            parent_scope: shared.scope.clone(),
            scope: package_scope,
            timeout: node.timeout(),
            slot_blocks: match node {
                Node::Flow(n) => n.slots.clone(),
                _ => None,
            },
            inputs_def_patch: node.inputs_def_patch(),
            path_finder: shared.path_finder.clone(),
        }
    });

    tracing::info!("run node {} as job {job_id}", node.node_id());

    if let Some(handle) = handle {
        ctx.jobs.insert(
            job_id,
            BlockInFlowJobHandle {
                node_id: node.node_id().to_owned(),
                _job: handle,
            },
        );
    } else {
        warn!("node: {} has no handle", node.node_id());
    }
}

fn is_finish(ctx: &RunFlowContext) -> bool {
    ctx.jobs.is_empty()
}

pub fn get_flow_cache_path(flow: &str) -> Option<PathBuf> {
    utils::cache::cache_meta_file_path().and_then(|path| {
        CacheMetaMap::load(path)
            .ok()
            .and_then(|meta| meta.get(flow).map(|cache_path| cache_path.into()))
    })
}

fn save_flow_cache(node_input_values: &NodeInputValues, flow: &str) {
    if let Some(cache_path) = get_flow_cache_path(flow) {
        if let Err(e) = node_input_values.save_last_value(cache_path) {
            warn!("failed to save cache: {}", e);
        }
    } else if let Some(meta_path) = utils::cache::cache_meta_file_path() {
        let cache_path = utils::cache::cache_dir()
            .unwrap_or(std::env::temp_dir())
            .join(Uuid::new_v4().to_string() + ".json");

        if let Err(e) = node_input_values.save_last_value(cache_path.clone()) {
            warn!("failed to save cache: {}", e);
            return;
        }

        let mut meta = CacheMetaMap::load(meta_path.clone()).unwrap_or_default();
        if let Some(path) = cache_path.to_str() {
            meta.insert(flow.to_owned(), path.to_string());
            if let Err(e) = meta.save(meta_path) {
                warn!("failed to save cache meta: {}", e);
            }
        }
    }
}
