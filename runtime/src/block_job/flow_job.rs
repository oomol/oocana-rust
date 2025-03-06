use std::{collections::HashMap, sync::Arc};

use manifest_meta::{FlowBlock, HandleName, HandleTo, JsonValue, Node, NodeId};

use crate::{
    block_status::{self, BlockStatusTx},
    shared::Shared,
};
use job::{BlockInputs, BlockJobStacks, JobId};

mod node_input_values;
use node_input_values::{InputValues, NodeInputValues};

mod run_to_node;
use run_to_node::RunToNode;

use super::{run_block, BlockJobHandle};

pub struct FlowJobHandle {
    pub job_id: JobId,
    _spawn_handle: tokio::task::JoinHandle<()>,
}

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

pub fn run_flow_block(
    flow_block: Arc<FlowBlock>, shared: Arc<Shared>, stacks: BlockJobStacks, flow_job_id: JobId,
    inputs: Option<BlockInputs>, parent_block_status: BlockStatusTx, to_node: Option<NodeId>,
) -> Option<BlockJobHandle> {
    let reporter = Arc::new(shared.reporter.block(
        flow_job_id.to_owned(),
        Some(flow_block.path_str.clone()),
        stacks.clone(),
    ));
    reporter.started();

    let (block_status_tx, block_status_rx) = block_status::create();

    let run_to_node = RunToNode::new(&flow_block, to_node);

    let flow_shared = FlowShared {
        flow_block,
        job_id: flow_job_id.to_owned(),
        shared,
        stacks,
    };

    let mut run_flow_ctx = RunFlowContext {
        node_input_values: NodeInputValues::new(),
        parent_block_status,
        jobs: HashMap::new(),
        block_status: block_status_tx,
    };

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
                );
            }
        }
    }

    for node in flow_shared.flow_block.nodes.values() {
        if run_to_node.should_run(node.node_id()) {
            if should_node_first_start(node, run_flow_ctx.node_input_values.peek(node.node_id())) {
                run_node(node, &flow_shared, &mut run_flow_ctx);
            }
        }
    }

    if check_done(&flow_shared, &run_flow_ctx) {
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
                                    );
                                }
                            }
                        }
                        if done {
                            run_flow_ctx.jobs.remove(&job_id);
                            if check_done(&flow_shared, &run_flow_ctx) {
                                break;
                            }
                        }
                    }
                }
                block_status::Status::Done { job_id } => {
                    run_flow_ctx.jobs.remove(&job_id);
                    if check_done(&flow_shared, &run_flow_ctx) {
                        break;
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

fn should_node_first_start(node: &Node, input_values: Option<&InputValues>) -> bool {
    if let Some(inputs_def) = node.inputs_def() {
        for handle in inputs_def.values() {
            if handle.cache.has_value() && !node.has_from(&handle.handle) {
                continue;
            }
            if !handle.optional || node.has_from(&handle.handle) {
                if let Some(input_values) = input_values {
                    if input_values.get(&handle.handle).is_none() {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
    }
    true
}

fn should_node_start(
    node: &Node, triggered_handle: Option<&HandleName>, input_values: Option<&InputValues>,
) -> bool {
    if let Some(inputs_def) = node.inputs_def() {
        if let Some(handle) = triggered_handle {
            if let Some(handle_def) = inputs_def.get(handle) {
                if !handle_def.trigger {
                    return false;
                }
            }
        }
        for handle in inputs_def.values() {
            if !handle.cache.has_value() {
                if !handle.optional || node.has_from(&handle.handle) {
                    if let Some(input_values) = input_values {
                        if input_values.get(&handle.handle).is_none() {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn produce_new_value(
    value: &Arc<JsonValue>, handle_tos: &Vec<HandleTo>, shared: &FlowShared,
    ctx: &mut RunFlowContext, run_next_node: bool, run_to_node: &RunToNode,
) {
    for handle_to in handle_tos {
        match handle_to {
            HandleTo::ToNodeInput {
                node_id,
                node_input_handle,
            } => {
                if run_to_node.should_run(node_id) {
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
                        if should_node_start(
                            node,
                            Some(node_input_handle),
                            ctx.node_input_values.peek(node_id),
                        ) {
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

fn check_done(shared: &FlowShared, ctx: &RunFlowContext) -> bool {
    if ctx.jobs.is_empty() {
        ctx.parent_block_status.done(shared.job_id.to_owned());
        true
    } else {
        false
    }
}
