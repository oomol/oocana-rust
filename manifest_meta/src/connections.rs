use std::collections::{HashMap, HashSet};

use manifest_reader::manifest::{self};

use super::{
    node::{HandleFrom, HandleTo},
    HandleName, HandlesFroms, HandlesTos, NodeId, NodesHandlesFroms, NodesHandlesTos,
};

pub struct Connections {
    pub nodes: HashSet<NodeId>,

    pub node_inputs_froms: ConnNodesFroms,
    pub node_outputs_tos: ConnNodesTos,

    pub flow_inputs_tos: ConnNodeTos,
    pub flow_outputs_froms: ConnNodeFroms,

    pub slot_inputs_tos: ConnSlotNodesTos,
    pub slot_outputs_froms: ConnSlotNodesFroms,
}

impl Connections {
    pub fn new(nodes: HashSet<NodeId>) -> Self {
        Self {
            nodes,
            node_inputs_froms: ConnNodesFroms::new(),
            node_outputs_tos: ConnNodesTos::new(),

            flow_inputs_tos: ConnNodeTos::new(),
            flow_outputs_froms: ConnNodeFroms::new(),

            slot_inputs_tos: ConnSlotNodesTos::new(),
            slot_outputs_froms: ConnSlotNodesFroms::new(),
        }
    }

    pub fn parse_flow_outputs_from(&mut self, outputs_from: Option<Vec<manifest::NodeInputFrom>>) {
        if let Some(outputs_from) = outputs_from {
            for output_from in outputs_from {
                if let Some(from_nodes) = output_from.from_node {
                    for from_node in from_nodes {
                        if !self.nodes.contains(&from_node.node_id) {
                            continue;
                        }

                        self.flow_outputs_froms.add(
                            output_from.handle.to_owned(),
                            HandleFrom::FromNodeOutput {
                                node_id: from_node.node_id.to_owned(),
                                node_output_handle: from_node.output_handle.to_owned(),
                            },
                        );
                        self.node_outputs_tos.add(
                            from_node.node_id.to_owned(),
                            from_node.output_handle.to_owned(),
                            HandleTo::ToFlowOutput {
                                output_handle: output_from.handle.to_owned(),
                            },
                        );
                    }
                }

                if let Some(from_subflows) = output_from.from_flow {
                    for from_subflow in from_subflows {
                        self.flow_outputs_froms.add(
                            output_from.handle.to_owned(),
                            HandleFrom::FromFlowInput {
                                input_handle: from_subflow.input_handle.to_owned(),
                            },
                        );
                        self.flow_inputs_tos.add(
                            from_subflow.input_handle.to_owned(),
                            HandleTo::ToFlowOutput {
                                output_handle: output_from.handle.to_owned(),
                            },
                        );
                    }
                }

                if let Some(from_slot_nodes) = output_from.from_slot {
                    for from_slot_node in from_slot_nodes {
                        if !self.nodes.contains(&from_slot_node.slot_node_id) {
                            continue;
                        }
                        self.flow_outputs_froms.add(
                            output_from.handle.to_owned(),
                            HandleFrom::FromSlotInput {
                                subflow_node_id: from_slot_node.subflow_node_id.to_owned(),
                                slot_node_id: from_slot_node.slot_node_id.to_owned(),
                                slot_input_handle: from_slot_node.input_handle.to_owned(),
                            },
                        );
                        self.slot_inputs_tos.add(
                            from_slot_node.subflow_node_id.to_owned(),
                            from_slot_node.slot_node_id.to_owned(),
                            from_slot_node.input_handle.to_owned(),
                            HandleTo::ToFlowOutput {
                                output_handle: output_from.handle.to_owned(),
                            },
                        );
                    }
                }
            }
        }
    }

    pub fn parse_node_inputs_from(
        &mut self,
        node_id: &NodeId,
        inputs_from: Option<&Vec<manifest::NodeInputFrom>>,
    ) {
        if let Some(inputs_from) = inputs_from {
            for input_from in inputs_from {
                if let Some(from_nodes) = &input_from.from_node {
                    for from_node in from_nodes {
                        // 连接的节点不在当前 flow 中，不创建连线
                        if !self.nodes.contains(&from_node.node_id) {
                            tracing::warn!(
                                "Node {} input {} from node {} not in nodes",
                                node_id,
                                input_from.handle,
                                from_node.node_id
                            );
                            continue;
                        }

                        self.node_inputs_froms.add(
                            node_id.to_owned(),
                            input_from.handle.to_owned(),
                            HandleFrom::FromNodeOutput {
                                node_id: from_node.node_id.to_owned(),
                                node_output_handle: from_node.output_handle.to_owned(),
                            },
                        );
                        self.node_outputs_tos.add(
                            from_node.node_id.to_owned(),
                            from_node.output_handle.to_owned(),
                            HandleTo::ToNodeInput {
                                node_id: node_id.to_owned(),
                                node_input_handle: input_from.handle.to_owned(),
                            },
                        );
                    }
                }

                if let Some(from_subflows) = &input_from.from_flow {
                    for from_subflow in from_subflows {
                        self.node_inputs_froms.add(
                            node_id.to_owned(),
                            input_from.handle.to_owned(),
                            HandleFrom::FromFlowInput {
                                input_handle: from_subflow.input_handle.to_owned(),
                            },
                        );
                        self.flow_inputs_tos.add(
                            from_subflow.input_handle.to_owned(),
                            HandleTo::ToNodeInput {
                                node_id: node_id.to_owned(),
                                node_input_handle: input_from.handle.to_owned(),
                            },
                        );
                    }
                }

                if let Some(from_slot_nodes) = &input_from.from_slot {
                    for from_slot_node in from_slot_nodes {
                        self.node_inputs_froms.add(
                            node_id.to_owned(),
                            input_from.handle.to_owned(),
                            HandleFrom::FromSlotInput {
                                subflow_node_id: from_slot_node.subflow_node_id.to_owned(),
                                slot_node_id: from_slot_node.slot_node_id.to_owned(),
                                slot_input_handle: from_slot_node.input_handle.to_owned(),
                            },
                        );
                        self.slot_inputs_tos.add(
                            from_slot_node.subflow_node_id.to_owned(),
                            from_slot_node.slot_node_id.to_owned(),
                            from_slot_node.input_handle.to_owned(),
                            HandleTo::ToNodeInput {
                                node_id: node_id.to_owned(),
                                node_input_handle: input_from.handle.to_owned(),
                            },
                        );
                    }
                }
            }
        }
    }

    pub fn parse_subflow_slot_outputs_from(
        &mut self,
        subflow_node_id: &NodeId,
        slots: Option<&Vec<manifest::SubflowNodeSlots>>,
    ) {
        if let Some(slots) = slots {
            for slot in slots {
                for output_from in &slot.outputs_from {
                    if let Some(from_nodes) = &output_from.from_node {
                        for from_node in from_nodes {
                            self.slot_outputs_froms.add(
                                subflow_node_id.to_owned(),
                                slot.slot_node_id.to_owned(),
                                output_from.handle.to_owned(),
                                HandleFrom::FromNodeOutput {
                                    node_id: from_node.node_id.to_owned(),
                                    node_output_handle: from_node.output_handle.to_owned(),
                                },
                            );
                            self.node_outputs_tos.add(
                                from_node.node_id.to_owned(),
                                from_node.output_handle.to_owned(),
                                HandleTo::ToSlotOutput {
                                    subflow_node_id: subflow_node_id.to_owned(),
                                    slot_node_id: slot.slot_node_id.to_owned(),
                                    slot_output_handle: output_from.handle.to_owned(),
                                },
                            );
                        }
                    }

                    if let Some(from_subflows) = &output_from.from_flow {
                        for from_subflow in from_subflows {
                            self.slot_outputs_froms.add(
                                subflow_node_id.to_owned(),
                                slot.slot_node_id.to_owned(),
                                output_from.handle.to_owned(),
                                HandleFrom::FromFlowInput {
                                    input_handle: from_subflow.input_handle.to_owned(),
                                },
                            );
                            self.flow_inputs_tos.add(
                                from_subflow.input_handle.to_owned(),
                                HandleTo::ToSlotOutput {
                                    subflow_node_id: subflow_node_id.to_owned(),
                                    slot_node_id: slot.slot_node_id.to_owned(),
                                    slot_output_handle: output_from.handle.to_owned(),
                                },
                            );
                        }
                    }

                    if let Some(from_slot_nodes) = &output_from.from_slot {
                        for from_slot_node in from_slot_nodes {
                            self.slot_outputs_froms.add(
                                subflow_node_id.to_owned(),
                                slot.slot_node_id.to_owned(),
                                output_from.handle.to_owned(),
                                HandleFrom::FromSlotInput {
                                    subflow_node_id: from_slot_node.subflow_node_id.to_owned(),
                                    slot_node_id: from_slot_node.slot_node_id.to_owned(),
                                    slot_input_handle: from_slot_node.input_handle.to_owned(),
                                },
                            );
                            self.slot_inputs_tos.add(
                                from_slot_node.subflow_node_id.to_owned(),
                                from_slot_node.slot_node_id.to_owned(),
                                from_slot_node.input_handle.to_owned(),
                                HandleTo::ToSlotOutput {
                                    subflow_node_id: subflow_node_id.to_owned(),
                                    slot_node_id: slot.slot_node_id.to_owned(),
                                    slot_output_handle: output_from.handle.to_owned(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnNodeTos {
    node_tos: HashMap<HandleName, Vec<HandleTo>>,
}

impl ConnNodeTos {
    pub fn new() -> Self {
        Self {
            node_tos: HashMap::new(),
        }
    }

    pub fn add(&mut self, handle: HandleName, to: HandleTo) {
        self.node_tos.entry(handle).or_default().push(to);
    }

    pub fn restore(self) -> HandlesTos {
        self.node_tos
    }
}

impl Default for ConnNodeTos {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ConnNodeFroms {
    node_froms: HashMap<HandleName, Vec<HandleFrom>>,
}

impl ConnNodeFroms {
    pub fn new() -> Self {
        Self {
            node_froms: HashMap::new(),
        }
    }

    pub fn add(&mut self, handle: HandleName, from: HandleFrom) {
        self.node_froms.entry(handle).or_default().push(from);
    }

    pub fn restore(self) -> HandlesFroms {
        self.node_froms
    }
}

impl Default for ConnNodeFroms {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ConnNodesTos {
    nodes: HashMap<NodeId, ConnNodeTos>,
}

impl ConnNodesTos {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add(&mut self, node_id: NodeId, handle: HandleName, to: HandleTo) {
        self.nodes.entry(node_id).or_default().add(handle, to);
    }

    pub fn remove(&mut self, node_id: &NodeId) -> Option<HandlesTos> {
        self.nodes.remove(node_id).map(ConnNodeTos::restore)
    }

    pub fn restore(self) -> NodesHandlesTos {
        self.nodes
            .into_iter()
            .map(|(node_id, conn_node_tos)| (node_id, conn_node_tos.restore()))
            .collect()
    }
}

impl Default for ConnNodesTos {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ConnNodesFroms {
    nodes: HashMap<NodeId, ConnNodeFroms>,
}

impl ConnNodesFroms {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add(&mut self, node_id: NodeId, handle: HandleName, from: HandleFrom) {
        self.nodes.entry(node_id).or_default().add(handle, from);
    }

    pub fn remove(&mut self, node_id: &NodeId) -> Option<HandlesFroms> {
        self.nodes.remove(node_id).map(ConnNodeFroms::restore)
    }

    pub fn restore(self) -> NodesHandlesFroms {
        self.nodes
            .into_iter()
            .map(|(node_id, conn_node_froms)| (node_id, conn_node_froms.restore()))
            .collect()
    }
}

impl Default for ConnNodesFroms {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ConnSlotNodesFroms {
    subflow_nodes: HashMap<NodeId, ConnNodesFroms>,
}

impl ConnSlotNodesFroms {
    pub fn new() -> Self {
        Self {
            subflow_nodes: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        subflow_node_id: NodeId,
        node_id: NodeId,
        handle: HandleName,
        from: HandleFrom,
    ) {
        self.subflow_nodes
            .entry(subflow_node_id)
            .or_default()
            .add(node_id, handle, from);
    }

    pub fn remove(&mut self, subflow_node_id: &NodeId) -> Option<NodesHandlesFroms> {
        self.subflow_nodes
            .remove(subflow_node_id)
            .map(ConnNodesFroms::restore)
    }
}

impl Default for ConnSlotNodesFroms {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ConnSlotNodesTos {
    subflow_nodes: HashMap<NodeId, ConnNodesTos>,
}

impl ConnSlotNodesTos {
    pub fn new() -> Self {
        Self {
            subflow_nodes: HashMap::new(),
        }
    }

    pub fn add(
        &mut self,
        subflow_node_id: NodeId,
        node_id: NodeId,
        handle: HandleName,
        to: HandleTo,
    ) {
        self.subflow_nodes
            .entry(subflow_node_id)
            .or_default()
            .add(node_id, handle, to);
    }

    pub fn remove(&mut self, subflow_node_id: &NodeId) -> Option<NodesHandlesTos> {
        self.subflow_nodes
            .remove(subflow_node_id)
            .map(ConnNodesTos::restore)
    }
}

impl Default for ConnSlotNodesTos {
    fn default() -> Self {
        Self::new()
    }
}
