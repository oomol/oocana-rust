use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use crate::{
    block_status::{self, BlockStatusTx},
    shared::Shared,
};
use mainframe::reporter::FlowReporterTx;
use utils::output::OutputValue;

use job::{BlockInputs, BlockJobStacks, JobId};
use manifest_meta::{FlowBlock, HandleFrom, HandleTo, Node, NodeId};

mod node_input_values;
use node_input_values::NodeInputValues;

mod run_to_node;
use run_to_node::RunToNode;

use super::{run_block, BlockJobHandle};

pub struct FlowJobHandle {
    pub job_id: JobId,
    _spawn_handle: tokio::task::JoinHandle<()>,
}

pub const LAST_VALUE_FILE: &str = "last_input_values.json";

struct BlockInFlowJobHandle {
    node_id: NodeId,
    _job: BlockJobHandle,
}

struct FlowShared {
    job_id: JobId,
    flow_block: Arc<FlowBlock>,
    shared: Arc<Shared>,
    stacks: BlockJobStacks,
}

struct RunFlowContext {
    node_input_values: NodeInputValues,
    parent_block_status: BlockStatusTx,
    jobs: HashMap<JobId, BlockInFlowJobHandle>,
    block_status: BlockStatusTx,
}

pub fn run_flow(
    flow_block: Arc<FlowBlock>, shared: Arc<Shared>, stacks: BlockJobStacks, flow_job_id: JobId,
    inputs: Option<BlockInputs>, parent_block_status: BlockStatusTx, to_node: Option<NodeId>,
    nodes: Option<HashSet<NodeId>>, input_values: Option<String>,
) -> Option<BlockJobHandle> {
    let reporter = Arc::new(shared.reporter.flow(
        flow_job_id.to_owned(),
        Some(flow_block.path_str.clone()),
        stacks.clone(),
    ));
    reporter.started(&inputs);

    let (block_status_tx, block_status_rx) = block_status::create();

    let run_to_node = RunToNode::new(&flow_block, to_node);

    let flow_shared = FlowShared {
        flow_block,
        job_id: flow_job_id.to_owned(),
        shared,
        stacks,
    };

    let mut nodes_that_only_should_be_run = nodes.clone();

    let mut run_flow_ctx = if nodes.is_some() {
        RunFlowContext {
            node_input_values: NodeInputValues::recover_from(LAST_VALUE_FILE.into()),
            parent_block_status,
            jobs: HashMap::new(),
            block_status: block_status_tx,
        }
    } else {
        RunFlowContext {
            node_input_values: NodeInputValues::new(),
            parent_block_status,
            jobs: HashMap::new(),
            block_status: block_status_tx,
        }
    };

    if let Some(node_input_values) = input_values {
        run_flow_ctx
            .node_input_values
            .merge_input_values(node_input_values);
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
                    &run_to_node,
                    &nodes,
                );
            }
        }
    }

    if let Some(ref nodes) = nodes {
        let (node_not_found, out_of_side_nodes, node_can_run) =
            calc_nodes(nodes, &flow_shared, &mut run_flow_ctx);

        if node_not_found.len() > 0 {
            let not_found_message = node_not_found
                .iter()
                .map(|node_id| node_id.to_string())
                .collect::<Vec<String>>()
                .join(",");
            println!("some nodes are not found: {}", not_found_message);
        }

        for node in node_can_run {
            run_node(node, &flow_shared, &mut run_flow_ctx);
        }

        for node_id in out_of_side_nodes {
            if let Some(node) = flow_shared.flow_block.nodes.get(&node_id) {
                nodes_that_only_should_be_run.as_mut().map(|f| {
                    f.insert(node_id.to_owned());
                });
                if run_flow_ctx.node_input_values.is_node_fulfill(node) {
                    println!("run outside node: {}", node.node_id());
                    run_node(node, &flow_shared, &mut run_flow_ctx);
                }
            }
        }
    } else {
        for node in flow_shared.flow_block.nodes.values() {
            if run_to_node.should_run(node.node_id()) {
                if run_flow_ctx.node_input_values.is_node_fulfill(&node) {
                    run_node(node, &flow_shared, &mut run_flow_ctx);
                }
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
                                        &run_to_node,
                                        &nodes_that_only_should_be_run,
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
                    if let Some(error) = error {
                        // 单独处理 done with error ，方便添加配置来决定是否继续执行 Flow。
                        run_flow_ctx.jobs.clear();
                        run_flow_ctx
                            .parent_block_status
                            .done(flow_shared.job_id.to_owned(), Some(error));
                        break;
                    } else {
                        run_flow_ctx.jobs.remove(&job_id);
                        if check_done(&flow_shared, &run_flow_ctx, &reporter) {
                            break;
                        }
                    }
                }
            };
        }
    });

    Some(BlockJobHandle::new(
        flow_job_id.to_owned(),
        FlowJobHandle {
            job_id: flow_job_id,
            _spawn_handle: spawn_handle,
        },
    ))
}

fn calc_nodes<'a>(
    nodes: &HashSet<NodeId>, flow_shared: &'a FlowShared, run_flow_ctx: &mut RunFlowContext,
) -> (HashSet<NodeId>, HashSet<NodeId>, Vec<&'a Node>) {
    let mut node_id_not_found = HashSet::new();
    let mut dep_node_id_outside_list = HashSet::new();
    let mut node_will_run = Vec::new();

    for node_id in nodes {
        if let Some(node) = flow_shared.flow_block.nodes.get(&node_id) {
            let n = RunToNode::new(&flow_shared.flow_block, Some(node_id.to_owned()));

            let nodes_without_self = nodes
                .iter()
                .filter(|id| *id != node_id)
                .map(|id| id.to_owned())
                .collect();

            let is_fulfill = run_flow_ctx.node_input_values.is_node_fulfill(node);

            // 如果有依赖的节点在 nodes 列表中，删除在 input value 中的数据，让列表中的依赖节点，来触发该 node 的运行
            if n.has_deps_in(&nodes_without_self) {
                let dep_in_nodes: HashSet<NodeId> = n
                    .find_intersection_nodes(&nodes_without_self)
                    .iter()
                    .map(|id| id.to_owned())
                    .collect();

                run_flow_ctx
                    .node_input_values
                    .remove_input_values(node, &dep_in_nodes);
            }

            if run_flow_ctx.node_input_values.is_node_fulfill(node) {
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
    value: &Arc<OutputValue>, handle_tos: &Vec<HandleTo>, shared: &FlowShared,
    ctx: &mut RunFlowContext, run_next_node: bool, run_to_node: &RunToNode,
    run_nodes: &Option<HashSet<NodeId>>,
) {
    for handle_to in handle_tos {
        match handle_to {
            HandleTo::ToNodeInput {
                node_id,
                node_input_handle,
            } => {
                if run_to_node.should_run(node_id)
                    && (run_nodes.as_ref().is_none()
                        || run_nodes
                            .as_ref()
                            .is_some_and(|nodes| nodes.contains(node_id)))
                {
                    ctx.node_input_values.insert(
                        node_id.to_owned(),
                        node_input_handle.to_owned(),
                        Arc::clone(value),
                    );

                    let node = shared
                        .flow_block
                        .nodes
                        .get(node_id)
                        .expect("Node not found");
                    if run_next_node {
                        if ctx.node_input_values.is_node_fulfill(node) {
                            _ = run_node(node, &shared, ctx);
                        }
                    }
                }
            }
            HandleTo::ToFlowOutput { flow_output_handle } => {
                ctx.parent_block_status.result(
                    shared.job_id.to_owned(),
                    Arc::clone(value),
                    flow_output_handle.to_owned(),
                    false,
                );
            }
            HandleTo::ToSlotOutput {
                flow_node_id,
                slot_node_id,
                slot_output_handle,
            } => {
                dbg!(flow_node_id, slot_node_id, slot_output_handle);
                todo!("ToSlotOutput");
            }
        }
    }
}

fn run_node(node: &Node, shared: &FlowShared, ctx: &mut RunFlowContext) {
    let job_id = JobId::random();
    let handle = run_block(
        node.block(),
        Arc::clone(&shared.shared),
        Some(Arc::clone(&shared.flow_block)),
        shared.stacks.stack(
            shared.job_id.to_owned(),
            shared.flow_block.path_str.to_owned(),
            node.node_id().to_owned(),
        ),
        job_id.clone(),
        ctx.node_input_values.take(node),
        ctx.block_status.clone(),
        None,
        None,
        None,
    );

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

        ctx.node_input_values
            .save_last_value(LAST_VALUE_FILE.into())
            .unwrap_or_else(|e| {
                eprintln!("failed to save last input values: {}", e);
            });
        true
    } else {
        false
    }
}
