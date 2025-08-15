use std::{
    collections::{HashMap, HashSet},
    default,
    path::PathBuf,
    sync::Arc,
};
use uuid::Uuid;

use crate::{
    block_job::{execute_task_job, BlockJobHandle, TaskJobParameters},
    block_status::{self, BlockStatusTx},
    flow_job::block_request::{
        parse_node_downstream, parse_query_block_request, parse_run_block_request,
        RunBlockSuccessResponse,
    },
    run::{run_job, CommonJobParameters, JobParams},
    shared::Shared,
};
use mainframe::{
    reporter::FlowReporterTx,
    scheduler::{
        self, BlockRequest, BlockResponseParams, OutputOptions, QueryBlockRequest, ToFlowOutput,
        ToNodeInput,
    },
};
use tracing::warn;
use utils::output::OutputValue;

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use manifest_meta::{
    Block, BlockResolver, BlockScope, HandleTo, InputHandle, Node, NodeId, Slot, SubflowBlock,
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
    parent_scope: RuntimeScope,
    scope: RuntimeScope,
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

pub struct FlowJobParameters {
    pub flow_block: Arc<SubflowBlock>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub flow_job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub node_value_store: NodeInputValues,
    pub parent_block_status: BlockStatusTx,
    pub nodes: Option<HashSet<NodeId>>,
    pub parent_scope: RuntimeScope,
    pub scope: RuntimeScope,
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

pub fn execute_flow_job(mut params: FlowJobParameters) -> Option<BlockJobHandle> {
    let FlowJobParameters {
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
    } = params;

    let reporter = Arc::new(shared.reporter.flow(
        flow_job_id.to_owned(),
        Some(flow_block.path_str.clone()),
        stacks.clone(),
    ));
    reporter.started(&inputs);
    let absence_node_inputs = flow_block
        .query_nodes_inputs()
        .into_iter()
        .filter_map(|(node_id, handles)| {
            if flow_block
                .nodes
                .get(&node_id)
                .is_none_or(|n| node_value_store.is_node_fulfill(n))
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

    if !absence_node_inputs.is_empty() {
        let node_and_handles = absence_node_inputs
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

    // Build limit_nodes: if Some, run only those nodes and all their upstream dependencies; None means run all nodes.
    let mut limit_nodes = nodes.clone();
    let mut run_flow_ctx = RunFlowContext {
        node_input_values: node_value_store,
        parent_block_status,
        jobs: HashMap::new(),
        block_status: block_status_tx,
        node_queue_pool: HashMap::new(),
    };

    let flow_path = flow_block.path.clone();
    let flow_shared = FlowShared {
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
            limit_nodes
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
                    &limit_nodes,
                    &reporter,
                    &None,
                );
            }
        }
    }

    if is_finish(&run_flow_ctx) {
        flow_success(&flow_shared, &run_flow_ctx, &reporter);
        return None;
    }

    struct EstimationNodeProgress {
        progress: f32,
        weight: f32,
    }

    let mut estimation_node_progress_store = HashMap::new();
    let mut total_weight = 0.0;
    for (node_id, node) in flow_shared.flow_block.nodes.iter() {
        estimation_node_progress_store.insert(
            node_id.clone(),
            EstimationNodeProgress {
                progress: 0.0,
                weight: node.progress_weight(),
            },
        );
        total_weight += node.progress_weight();
    }
    let mut estimation_progress_sum = 0.0; // 0.0 ~ 100.0

    let mut update_node_progress = move |progress: f32,
                                         estimation_node_progress: &mut EstimationNodeProgress|
          -> Option<f32> {
        if estimation_node_progress.weight == 0.0
            || estimation_node_progress.progress >= 100.0
            || (progress - estimation_node_progress.progress).abs() < f32::EPSILON
        {
            return None;
        }

        let new_progress = f32::max(estimation_node_progress.progress, progress).clamp(0.0, 100.0);

        let old_weight_progress =
            estimation_node_progress.progress * estimation_node_progress.weight;

        estimation_node_progress.progress = new_progress;
        let new_weight_progress = new_progress * estimation_node_progress.weight;

        estimation_progress_sum += new_weight_progress - old_weight_progress;

        let estimation_flow_progress = if total_weight > 0.0 {
            estimation_progress_sum / total_weight
        } else {
            return None;
        };
        // we calculate the estimation flow progress by node finish, but node can run multiple times, leave it to 95% at most.
        Some(estimation_flow_progress.clamp(0.0, 95.0))
    };

    let scheduler_tx = flow_shared.shared.scheduler_tx.clone();
    let mut block_resolver = BlockResolver::new();
    let mut flow_path_finder = flow_shared.path_finder.clone();
    let spawn_handle = tokio::spawn(async move {
        while let Some(status) = block_status_rx.recv().await {
            match status {
                block_status::Status::Output {
                    job_id,
                    result,
                    handle,
                    options,
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
                                        &limit_nodes,
                                        &reporter,
                                        &options,
                                    );
                                }
                            }
                        }
                    }
                }
                block_status::Status::Progress { job_id, progress } => {
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        if flow_shared.flow_block.nodes.contains_key(&job.node_id) {
                            let node_weight_progress =
                                estimation_node_progress_store.get_mut(&job.node_id);

                            if let Some(node_weight_progress) = node_weight_progress {
                                if let Some(flow_progress) =
                                    update_node_progress(progress, node_weight_progress)
                                {
                                    run_flow_ctx
                                        .parent_block_status
                                        .progress(flow_shared.job_id.to_owned(), flow_progress);
                                    reporter.progress(flow_progress);
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
                                            &limit_nodes,
                                            &reporter,
                                            &None,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                block_status::Status::Request(request) => match request {
                    BlockRequest::RunBlock(request) => {
                        let res = parse_run_block_request(
                            &request,
                            &mut block_resolver,
                            &mut flow_path_finder,
                            flow_shared.shared.clone(),
                            flow_shared.scope.clone(),
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
                                    if let Some(handle) = execute_task_job(TaskJobParameters {
                                        task_block,
                                        inputs_def,
                                        outputs_def,
                                        shared: flow_shared.shared.clone(),
                                        parent_flow: Some(flow_shared.flow_block.clone()),
                                        stacks: request_stack.stack(
                                            flow_shared.job_id.to_owned(),
                                            flow_shared.flow_block.path_str.to_owned(),
                                            node_id.clone(),
                                        ),
                                        job_id: job_id.clone(),
                                        inputs: Some(inputs),
                                        block_status: run_flow_ctx.block_status.clone(),
                                        scope,
                                        timeout: None,
                                        inputs_def_patch: None,
                                    }) {
                                        run_flow_ctx.jobs.insert(
                                            job_id.to_owned(),
                                            BlockInFlowJobHandle {
                                                node_id,
                                                _job: handle,
                                            },
                                        );
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
                                    if let Some(handle) = execute_flow_job(FlowJobParameters {
                                        flow_block,
                                        shared: flow_shared.shared.clone(),
                                        stacks: request_stack.stack(
                                            flow_shared.job_id.to_owned(),
                                            flow_shared.flow_block.path_str.to_owned(),
                                            node_id.clone(),
                                        ),
                                        flow_job_id: job_id.clone(),
                                        inputs: Some(inputs),
                                        node_value_store: NodeInputValues::new(false),
                                        parent_block_status: run_flow_ctx.block_status.clone(),
                                        nodes: None,
                                        parent_scope: flow_shared.scope.clone(),
                                        scope,
                                        slot_blocks: default::Default::default(),
                                        path_finder: flow_shared.path_finder.clone(),
                                    }) {
                                        run_flow_ctx.jobs.insert(
                                            job_id.to_owned(),
                                            BlockInFlowJobHandle {
                                                node_id,
                                                _job: handle,
                                            },
                                        );
                                    }
                                }
                            },
                            Err(err) => {
                                let msg =
                                    format!("Run block failed: {}. Block: {}", err, request.block);
                                tracing::warn!("{}", msg);
                                scheduler_tx.respond_block_request(
                                    &flow_shared.shared.session_id,
                                    scheduler::BlockResponseParams {
                                        session_id: flow_shared.shared.session_id.clone(),
                                        job_id: request.job_id.clone(),
                                        error: Some(msg),
                                        result: None,
                                        request_id: request.request_id,
                                    },
                                );
                            }
                        }
                    }
                    BlockRequest::QueryBlock(request) => {
                        let res = parse_query_block_request(
                            &request,
                            &mut block_resolver,
                            &mut flow_path_finder,
                        );
                        let QueryBlockRequest {
                            session_id,
                            job_id,
                            request_id,
                            ..
                        } = request;
                        match res {
                            Ok(json) => {
                                scheduler_tx.respond_block_request(
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
                                scheduler_tx.respond_block_request(
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
                        job_id,
                        outputs,
                        request_id,
                        session_id,
                    } => {
                        let node = run_flow_ctx
                            .jobs
                            .get(&job_id)
                            .map(|job| job.node_id.to_owned())
                            .and_then(|id| flow_shared.flow_block.nodes.get(&id));
                        let res = parse_node_downstream(
                            node,
                            &flow_shared.flow_block.nodes,
                            &outputs,
                            &flow_shared.flow_block.outputs_def,
                        );
                        match res {
                            Ok(json) => {
                                scheduler_tx.respond_block_request(
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
                                tracing::warn!("Query downstream failed: {}.", err,);
                                scheduler_tx.respond_block_request(
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
                },
                block_status::Status::Done {
                    job_id,
                    result,
                    error,
                } => {
                    run_pending_node(job_id.to_owned(), &flow_shared, &mut run_flow_ctx);

                    let success_done = error.is_none();

                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        if let Some(node) = flow_shared.flow_block.nodes.get(&job.node_id) {
                            let node_weight_progress =
                                estimation_node_progress_store.get_mut(&job.node_id);

                            if let Some(node_weight_progress) = node_weight_progress {
                                if success_done {
                                    if let Some(flow_progress) =
                                        update_node_progress(100.0, node_weight_progress)
                                    {
                                        run_flow_ctx
                                            .parent_block_status
                                            .progress(flow_shared.job_id.to_owned(), flow_progress);
                                        reporter.progress(flow_progress);
                                    }
                                }
                            }

                            if let Some(tos) = node.to() {
                                for (handle, value) in result.unwrap_or_default().iter() {
                                    if let Some(handle_tos) = tos.get(handle) {
                                        produce_new_value(
                                            value,
                                            handle_tos,
                                            &flow_shared,
                                            &mut run_flow_ctx,
                                            true,
                                            &limit_nodes,
                                            &reporter,
                                            &None,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // error already handled in the block report
                    if let Some(err) = error {
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

                        // add error node stack
                        let error_stack = format!(
                            "{} failed",
                            node_id
                                .map(|n| format!("[node id: {n}]"))
                                .unwrap_or_else(|| format!("job_id: {}", job_id)),
                        );
                        reporter.done(&Some(error_stack.clone()));

                        if flow_shared.stacks.is_root() {
                            run_flow_ctx.parent_block_status.finish(
                                flow_shared.job_id.to_owned(),
                                None,
                                Some(format!("flow {} failed.", flow_shared.flow_block.path_str)),
                            );
                        } else {
                            run_flow_ctx.parent_block_status.finish(
                                flow_shared.job_id.to_owned(),
                                None,
                                Some(err),
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
    limit_nodes: &Option<HashSet<NodeId>>,
    reporter: &FlowReporterTx,
    options: &Option<OutputOptions>,
) {
    for handle_to in handle_tos {
        match handle_to {
            HandleTo::ToNodeInput {
                node_id,
                input_handle,
            } => {
                // only handle when options is some:
                // - if to_node_inputs is not in options(None), skip this handle_to processing
                // - if to_node_inputs is in options: check whether this handle_to is contained in to_node_inputs.
                //      if handle_to is in to_node_input, continue processing
                //      if not, skip this handle_to processing
                if options.as_ref().is_some_and(|op| {
                    op.target.as_ref().map_or(false, |t| {
                        t.to_node.as_ref().map_or(true, |to_nodes| {
                            !to_nodes.contains(&ToNodeInput {
                                node_id: node_id.to_owned(),
                                input_handle: input_handle.to_owned(),
                            })
                        })
                    })
                }) {
                    continue;
                }

                let last_fulfill_count = if let Some(node) = shared.flow_block.nodes.get(node_id) {
                    ctx.node_input_values.node_pending_fulfill(node)
                } else {
                    0
                };

                // if limit_nodes is Some, output should only send to these nodes
                if limit_nodes
                    .as_ref()
                    .is_some_and(|nodes| !nodes.contains(node_id))
                {
                    // if target node is not in running nodes, we just update cache value so that we can use these value in next run.
                    // Normally, cache values are only cached before the block is executed. This is an exception.
                    ctx.node_input_values.update_serializable_cache_value(
                        node_id,
                        input_handle,
                        Arc::clone(value),
                    );
                    continue;
                }

                ctx.node_input_values
                    .insert(node_id, input_handle, Arc::clone(value));

                if run_next_node {
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
                                if pending_fulfill > last_fulfill_count {
                                    node_queue.pending.insert(JobId::random());
                                    tracing::info!("node queue ({}) is full, add a pending job. current jobs count: {}, concurrency: {}",node_id,node_queue.jobs.len(),node.concurrency());
                                } else {
                                    tracing::info!(
                                        "Node ({}) has pending jobs; this input event will not trigger again as it did not fulfill more than before.",
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
                // Refer to the logic for handling ToNodeInput
                if options.as_ref().is_some_and(|op| {
                    op.target.as_ref().map_or(false, |t| {
                        t.to_flow.as_ref().map_or(true, |to_outputs| {
                            !to_outputs.contains(&ToFlowOutput {
                                output_handle: flow_output_handle.to_owned(),
                            })
                        })
                    })
                }) {
                    continue;
                }

                reporter.output(value.clone(), flow_output_handle);
                ctx.parent_block_status.output(
                    shared.job_id.to_owned(),
                    Arc::clone(value),
                    flow_output_handle.to_owned(),
                    None, // don't support specify nodes for flow output yet. only support limit whether output to flow output or not
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

    let block_scope = if matches!(node, Node::Slot(_)) {
        shared
            .slot_blocks
            .get(node.node_id())
            .map(|slot| slot.scope().clone())
            .unwrap_or_else(|| node.scope())
    } else {
        node.scope()
    };

    let runtime_scope = match block_scope {
        BlockScope::Package {
            name,
            path,
            node_id,
            ..
        } => RuntimeScope {
            session_id: shared.scope.session_id.clone(),
            pkg_name: Some(name.clone()),
            path: path.clone(),
            data_dir: shared
                .scope
                .pkg_root
                .join(name)
                .to_string_lossy()
                .to_string(),
            pkg_root: shared.scope.pkg_root.clone(),
            node_id: node_id.clone(),
            enable_layer: layer::feature_enabled(),
            is_inject: node.scope().is_inject(),
        },
        BlockScope::Flow { node_id, .. } => RuntimeScope {
            session_id: shared.scope.session_id.clone(),
            pkg_name: shared.scope.pkg_name.clone(),
            pkg_root: shared.scope.pkg_root.clone(),
            data_dir: shared.scope.data_dir.clone(),
            path: shared.scope.path().to_owned(),
            node_id: node_id.clone(),
            enable_layer: shared.scope.need_layer(),
            is_inject: node.scope().is_inject(),
        },
        BlockScope::Slot { .. } => RuntimeScope {
            session_id: shared.scope.session_id.clone(),
            pkg_name: shared.parent_scope.pkg_name.clone(),
            pkg_root: shared.parent_scope.pkg_root.clone(),
            data_dir: shared.parent_scope.data_dir.clone(),
            path: shared.parent_scope.path().to_owned(),
            node_id: None,
            enable_layer: shared.parent_scope.need_layer(),
            is_inject: node.scope().is_inject(),
        },
    };

    let common_job_params = CommonJobParameters {
        shared: shared.shared.clone(),
        stacks: shared.stacks.stack(
            shared.job_id.to_owned(),
            shared.flow_block.path_str.to_owned(),
            node.node_id().to_owned(),
        ),
        job_id: job_id.clone(),
        inputs: ctx.node_input_values.take(node),
        block_status: ctx.block_status.clone(),
        scope: runtime_scope,
    };

    let job_params = match block {
        Block::Task(task_block) => JobParams::Task {
            inputs_def: node.inputs_def(),
            outputs_def: node.outputs_def().cloned(),
            task_block: task_block.clone(),
            parent_flow: Some(shared.flow_block.clone()),
            timeout: node.timeout(),
            inputs_def_patch: node.inputs_def_patch(),
            common: common_job_params,
        },
        Block::Flow(flow_block) => JobParams::Flow {
            flow_block: flow_block.clone(),
            nodes: None,
            parent_scope: shared.scope.clone(),
            node_value_store: NodeInputValues::new(false),
            slot_blocks: match node {
                Node::Flow(n) => n.slots.clone(),
                _ => None,
            },
            path_finder: shared.path_finder.clone(),
            common: common_job_params,
        },
        Block::Service(service_block) => JobParams::Service {
            service_block: service_block.clone(),
            parent_flow: Some(shared.flow_block.clone()),
            inputs_def_patch: node.inputs_def_patch(),
            common: common_job_params,
        },
        Block::Slot(slot_block) => JobParams::Slot {
            slot_block: slot_block.clone(),
            common: common_job_params,
        },
    };
    tracing::info!("run node {} as job {job_id}", node.node_id());

    if let Some(handle) = run_job(job_params) {
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
        if let Err(e) = node_input_values.save_cache(cache_path) {
            warn!("failed to save cache: {}", e);
        }
    } else if let Some(meta_path) = utils::cache::cache_meta_file_path() {
        let cache_path = utils::cache::cache_dir()
            .unwrap_or(std::env::temp_dir())
            .join(Uuid::new_v4().to_string() + ".json");

        if let Err(e) = node_input_values.save_cache(cache_path.clone()) {
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
