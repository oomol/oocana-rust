use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use uuid::Uuid;

use crate::{
    block_job::{run_block, BlockJobHandle, RunBlockArgs},
    block_status::{self, BlockStatusTx},
    shared::Shared,
};
use mainframe::reporter::FlowReporterTx;
use tracing::{info, warn};
use utils::output::OutputValue;

use job::{BlockInputs, BlockJobStacks, JobId};
use manifest_meta::{HandleFrom, HandleTo, Node, NodeId, SubflowBlock};

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
    pub parent_block_status: BlockStatusTx,
    pub nodes: Option<HashSet<NodeId>>,
    pub input_values: Option<String>,
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
    return (node_will_run, waiting_nodes, upstream_nodes);
}

pub fn run_flow(mut flow_args: RunFlowArgs) -> Option<BlockJobHandle> {
    let RunFlowArgs {
        flow_block,
        shared,
        stacks,
        flow_job_id,
        inputs,
        parent_block_status,
        ref mut nodes,
        input_values,
    } = flow_args;

    let reporter = Arc::new(shared.reporter.flow(
        flow_job_id.to_owned(),
        Some(flow_block.path_str.clone()),
        stacks.clone(),
    ));
    reporter.started(&inputs);

    let (block_status_tx, block_status_rx) = block_status::create();

    let mut filtered_nodes = nodes.clone();

    let flow_cache_path = if shared.use_cache {
        get_flow_cache_path(&flow_block.path_str)
    } else {
        None
    };

    // 暂时默认，不由外部参数来决定
    let save_cache = true;
    let mut run_flow_ctx = if shared.use_cache && flow_cache_path.is_some() {
        RunFlowContext {
            node_input_values: NodeInputValues::recover_from(flow_cache_path.unwrap(), save_cache),
            parent_block_status,
            jobs: HashMap::new(),
            block_status: block_status_tx,
            node_queue_pool: HashMap::new(),
        }
    } else {
        RunFlowContext {
            node_input_values: NodeInputValues::new(save_cache),
            parent_block_status,
            jobs: HashMap::new(),
            block_status: block_status_tx,
            node_queue_pool: HashMap::new(),
        }
    };

    if let Some(node_input_values) = input_values {
        run_flow_ctx
            .node_input_values
            .merge_input_values(node_input_values);
    }

    let flow_shared = FlowShared {
        flow_block,
        job_id: flow_job_id.to_owned(),
        shared,
        stacks,
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
            if run_flow_ctx.node_input_values.is_node_fulfill(&node) {
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
                    false,
                    &filtered_nodes,
                );
            }
        }
    }

    if check_done(&flow_shared, &run_flow_ctx, &reporter) {
        return None;
    }

    let spawn_handle = tokio::spawn(async move {
        while let Some(status) = block_status_rx.recv().await {
            match status {
                block_status::Status::Result {
                    job_id,
                    result,
                    handle,
                    done,
                } => {
                    if done {
                        run_pending_node(job_id.to_owned(), &flow_shared, &mut run_flow_ctx);
                    }
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        let job_node_id = job.node_id.to_owned();
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
                                    );
                                }
                            }
                        }

                        for froms in flow_shared.flow_block.flow_outputs_froms.values() {
                            for from in froms {
                                match from {
                                    HandleFrom::FromNodeOutput {
                                        node_id,
                                        node_output_handle,
                                    } => {
                                        if node_output_handle.eq(&handle) && node_id == &job_node_id
                                        {
                                            run_flow_ctx.parent_block_status.result(
                                                flow_shared.job_id.to_owned(),
                                                Arc::clone(&result),
                                                handle.to_owned(),
                                                done,
                                            );
                                            reporter.result(Arc::clone(&result), &handle)
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                block_status::Status::Done { job_id, error } => {
                    run_pending_node(job_id.to_owned(), &flow_shared, &mut run_flow_ctx);

                    if let Some(_) = error {
                        save_flow_cache(
                            &run_flow_ctx.node_input_values,
                            &flow_shared.flow_block.path_str,
                        );

                        let node_id = run_flow_ctx
                            .jobs
                            .get(&job_id)
                            .map(|job| job.node_id.to_owned());

                        run_flow_ctx.jobs.clear();

                        // 由于 block 失败中断时，只上报失败的节点 node id（找不到就报 job_id)。
                        // 在这里隐藏掉 error 信息，只上报 node id
                        run_flow_ctx.parent_block_status.done(
                            flow_shared.job_id.to_owned(),
                            Some(format!(
                                "{} failed",
                                node_id
                                    .map(|n| format!("node id: {n}"))
                                    .unwrap_or_else(|| format!("job_id: {}", job_id))
                            )),
                        );
                        break;
                    } else {
                        run_flow_ctx.jobs.remove(&job_id);
                        if check_done(&flow_shared, &run_flow_ctx, &reporter) {
                            break;
                        }
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

fn run_pending_node(job_id: JobId, flow_shared: &FlowShared, run_flow_ctx: &mut RunFlowContext) {
    if let Some(job_handle) = run_flow_ctx.jobs.get(&job_id) {
        let node_id = job_handle.node_id.to_owned();

        let node_queue = run_flow_ctx
            .node_queue_pool
            .get_mut(&node_id)
            .expect("Node queue not found");

        node_queue.jobs.remove(&job_id);

        if let Some(node) = flow_shared.flow_block.nodes.get(&node_id) {
            let concurrency = node.concurrency();
            let jobs_count = node_queue.jobs.len();
            if jobs_count < concurrency as usize && node_queue.pending.len() > 0 {
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
fn find_upstream_nodes<'a>(
    origin_nodes: &HashSet<NodeId>,
    flow_block: &'a SubflowBlock,
    node_input_values: &mut NodeInputValues,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let (node_not_found, out_of_side_nodes, node_can_run_directly) =
        calc_nodes(&origin_nodes, &flow_block, node_input_values);
    // 两部分：
    // 1. nodes 中可以直接 run 的
    // 2. nodes 中的依赖节点中可以直接 run 的节点
    let mut nodes_will_run = node_can_run_directly
        .iter()
        .map(|node| node.node_id().to_owned().into())
        .collect::<Vec<String>>();

    if node_not_found.len() > 0 {
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

    return (nodes_will_run, waiting_nodes, upstream_nodes);
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
        if let Some(node) = flow_block.nodes.get(&node_id) {
            let n = RunToNode::new(
                &flow_block,
                Some(node_id.to_owned()),
                Some(&node_input_values),
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

                let previous_pending_fulfill = ctx.node_input_values.node_pending_fulfill(node_id);
                // still need to insert value, even if the node is not in the run_nodes list
                ctx.node_input_values.insert(
                    node_id.to_owned(),
                    node_input_handle.to_owned(),
                    Arc::clone(value),
                );

                if should_run_output_node {
                    let node = shared
                        .flow_block
                        .nodes
                        .get(node_id)
                        .expect("Node not found");

                    if ctx.node_input_values.is_node_fulfill(node) {
                        let node_queue = ctx.node_queue_pool.entry(node_id.to_owned()).or_default();
                        if node_queue.jobs.len() < node.concurrency() as usize {
                            run_node(node, &shared, ctx);
                        } else {
                            // 说明这次数据填平了一次 pending
                            if ctx.node_input_values.node_pending_fulfill(node_id)
                                > previous_pending_fulfill
                            {
                                node_queue.pending.insert(JobId::random());
                            } else {
                                info!("node queue fulfill number is {}", previous_pending_fulfill);
                            }
                        }
                    }
                }
            }
            HandleTo::ToFlowOutput {
                output_handle: flow_output_handle,
            } => {
                ctx.parent_block_status.result(
                    shared.job_id.to_owned(),
                    Arc::clone(value),
                    flow_output_handle.to_owned(),
                    false,
                );
            }
            HandleTo::ToSlotOutput {
                subflow_node_id,
                slot_node_id,
                slot_output_handle,
            } => {
                dbg!(subflow_node_id, slot_node_id, slot_output_handle);
                todo!("ToSlotOutput");
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

    let handle = run_block({
        RunBlockArgs {
            block: node.block().to_owned(),
            shared: Arc::clone(&shared.shared),
            parent_flow: Some(Arc::clone(&shared.flow_block)),
            stacks: shared.stacks.stack(
                shared.job_id.to_owned(),
                shared.flow_block.path_str.to_owned(),
                node.node_id().to_owned(),
            ),
            job_id: job_id.to_owned(),
            inputs: ctx.node_input_values.take(node),
            block_status: ctx.block_status.clone(),
            nodes: None,
            input_values: None,
            scope: node.scope(),
            timeout_seconds: match node {
                Node::Task(task_node) => task_node.timeout_seconds,
                _ => None,
            },
            inputs_def_patch: node.inputs_def_patch().cloned(),
        }
    });

    if let Some(handle) = handle {
        ctx.jobs.insert(
            job_id,
            BlockInFlowJobHandle {
                node_id: node.node_id().to_owned(),
                _job: handle,
            },
        );
    }
}

fn check_done(shared: &FlowShared, ctx: &RunFlowContext, reporter: &FlowReporterTx) -> bool {
    if ctx.jobs.is_empty() {
        reporter.done(&None);
        ctx.parent_block_status.done(shared.job_id.to_owned(), None);
        save_flow_cache(&ctx.node_input_values, &shared.flow_block.path_str);
        true
    } else {
        false
    }
}

fn get_flow_cache_path(flow: &str) -> Option<PathBuf> {
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
