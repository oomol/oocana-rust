use std::{collections::HashMap, path::PathBuf};

use manifest_reader::block_manifest_reader::{block as manifest_block, node as manifest_node};
use utils::error::Result;

use crate::{
    block,
    block_reader::{BlockPathResolver, BlockReader},
    connections::Connections,
    node::AppletNode,
    FlowNode, HandlesFroms, HandlesTos, InputHandles, Node, NodeId, OutputHandles, SlotNode,
    TaskNode,
};

#[derive(Debug, Clone)]
pub struct FlowBlock {
    pub nodes: HashMap<NodeId, Node>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub path: PathBuf,
    pub path_str: String,
    /// Flow inputs to in-flow nodes
    pub flow_inputs_tos: HandlesTos,
    /// Flow outputs from in-flow nodes
    pub flow_outputs_froms: HandlesFroms,
}

impl FlowBlock {
    pub fn from_manifest(
        manifest: manifest_block::FlowBlock, flow_path: PathBuf, block_reader: &mut BlockReader,
        mut resolver: BlockPathResolver,
    ) -> Result<Self> {
        let manifest_block::FlowBlock {
            nodes,
            inputs_def,
            outputs_def,
            outputs_from,
        } = manifest;

        let mut connections = Connections::new();

        connections.parse_flow_outputs_from(outputs_from);

        for node in nodes.iter() {
            connections.parse_node_inputs_from(node.node_id(), node.inputs_from());
            if let manifest_node::Node::Flow(flow_node) = node {
                connections
                    .parse_subflow_slot_outputs_from(&flow_node.node_id, flow_node.slots.as_ref());
            }
        }

        let mut new_nodes: HashMap<NodeId, Node> = HashMap::new();
        for node in nodes {
            match node {
                manifest_node::Node::Flow(flow_node) => {
                    let flow = block_reader.resolve_flow_block(&flow_node.flow, &mut resolver)?;
                    let inputs_def =
                        parse_inputs_def(&flow_node.inputs_from, &flow.as_ref().inputs_def);

                    new_nodes.insert(
                        flow_node.node_id.to_owned(),
                        Node::Flow(FlowNode {
                            from: connections.node_inputs_froms.remove(&flow_node.node_id),
                            to: connections.node_outputs_tos.remove(&flow_node.node_id),
                            slots_outputs_from: connections
                                .slot_outputs_froms
                                .remove(&flow_node.node_id),
                            slots_inputs_to: connections.slot_inputs_tos.remove(&flow_node.node_id),
                            flow,
                            node_id: flow_node.node_id,
                            // title: flow_node.title,
                            timeout: flow_node.timeout,
                            inputs_def,
                        }),
                    );
                }
                manifest_node::Node::Applet(applet_node) => {
                    let applet = block_reader
                        .resolve_applet_node_block(applet_node.applet, &mut resolver)?;
                    let inputs_def =
                        parse_inputs_def(&applet_node.inputs_from, &applet.as_ref().inputs_def);

                    new_nodes.insert(
                        applet_node.node_id.to_owned(),
                        Node::Applet(AppletNode {
                            from: connections.node_inputs_froms.remove(&applet_node.node_id),
                            to: connections.node_outputs_tos.remove(&applet_node.node_id),
                            node_id: applet_node.node_id,
                            // title: flow_node.title,
                            timeout: applet_node.timeout,
                            block: applet,
                            inputs_def,
                        }),
                    );
                }
                manifest_node::Node::Task(task_node) => {
                    let task =
                        block_reader.resolve_task_node_block(task_node.task, &mut resolver)?;
                    let inputs_def =
                        parse_inputs_def(&task_node.inputs_from, &task.as_ref().inputs_def);

                    new_nodes.insert(
                        task_node.node_id.to_owned(),
                        Node::Task(TaskNode {
                            from: connections.node_inputs_froms.remove(&task_node.node_id),
                            to: connections.node_outputs_tos.remove(&task_node.node_id),
                            node_id: task_node.node_id,
                            // title: flow_node.title,
                            timeout: task_node.timeout,
                            task,
                            inputs_def,
                        }),
                    );
                }
                manifest_node::Node::Slot(slot_node) => {
                    let slot =
                        block_reader.resolve_slot_node_block(slot_node.slot, &mut resolver)?;
                    let inputs_def =
                        parse_inputs_def(&slot_node.inputs_from, &slot.as_ref().inputs_def);

                    new_nodes.insert(
                        slot_node.node_id.to_owned(),
                        Node::Slot(SlotNode {
                            from: connections.node_inputs_froms.remove(&slot_node.node_id),
                            to: connections.node_outputs_tos.remove(&slot_node.node_id),
                            node_id: slot_node.node_id,
                            // title: flow_node.title,
                            timeout: slot_node.timeout,
                            slot,
                            inputs_def,
                        }),
                    );
                }
            }
        }

        Ok(Self {
            nodes: new_nodes,
            inputs_def: block::to_input_handles(inputs_def),
            outputs_def: block::to_output_handles(outputs_def),
            path_str: flow_path.to_string_lossy().to_string(),
            path: flow_path,
            flow_inputs_tos: connections.flow_inputs_tos.restore(),
            flow_outputs_froms: connections.flow_outputs_froms.restore(),
        })
    }
}

fn parse_inputs_def(
    node_inputs_from: &Option<Vec<manifest_node::DataSource>>, inputs_def: &Option<InputHandles>,
) -> Option<InputHandles> {
    match inputs_def {
        Some(inputs_def) => {
            let mut merged_inputs_def = inputs_def.clone();
            if let Some(inputs_from) = node_inputs_from {
                for input in inputs_from {
                    // 如果 node_inputs_from 中的 input 不在 inputs_def 中，则不合并进 node 实例中的 inputs_def 中。这样可以避免后续做很多判断。
                    if !merged_inputs_def.contains_key(&input.handle) {
                        continue;
                    }

                    merged_inputs_def
                        .entry(input.handle.to_owned())
                        .and_modify(|def| {
                            if let Some(value) = &input.value {
                                def.value = Some(value.clone());
                            }
                        });
                }
            }

            Some(merged_inputs_def)
        }
        None => None,
    }
}
