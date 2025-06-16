use std::collections::{HashMap, HashSet};

use manifest_reader::manifest::{self, SlotProvider};

use crate::flow::generate_runtime_handle_name;

use super::{
    node::{HandleFrom, HandleTo},
    HandleName, HandlesFroms, HandlesTos, NodeId,
};

pub struct Connections {
    pub nodes: HashSet<NodeId>,

    pub node_inputs_froms: ConnNodesFroms,
    pub node_outputs_tos: ConnNodesTos,

    pub flow_inputs_tos: ConnNodeTos,
    pub flow_outputs_froms: ConnNodeFroms,
}

impl Connections {
    pub fn new(nodes: HashSet<NodeId>) -> Self {
        Self {
            nodes,
            node_inputs_froms: ConnNodesFroms::new(),
            node_outputs_tos: ConnNodesTos::new(),

            flow_inputs_tos: ConnNodeTos::new(),
            flow_outputs_froms: ConnNodeFroms::new(),
        }
    }

    /// add node connection from node to slot provider
    pub fn parse_slot_inputs_from(
        &mut self,
        subflow_node_id: &NodeId,
        slot_node_id: &NodeId,
        inputs_from: Vec<manifest::NodeInputFrom>,
        find_value_node: &impl Fn(&NodeId) -> Option<manifest::ValueNode>,
    ) {
        for slot_input_from in inputs_from {
            let runtime_handle =
                generate_runtime_handle_name(&slot_node_id, &slot_input_from.handle);
            if let Some(from_nodes) = slot_input_from.from_node {
                for from_node in from_nodes {
                    if let Some(value_node) = find_value_node(&from_node.node_id) {
                        if let Some(input) = value_node.get_handle(&from_node.output_handle) {
                            self.node_inputs_froms.add(
                                subflow_node_id.to_owned(),
                                runtime_handle.clone(),
                                HandleFrom::FromValue {
                                    value: input.value.clone(),
                                },
                            );
                            tracing::debug!(
                                "value node only add node_inputs_froms has no node_outputs_tos"
                            );
                            continue;
                        } else {
                            tracing::warn!(
                                "ignore slot connection because value node({}) has no output handle({})",
                                from_node.node_id,
                                from_node.output_handle
                            );
                        }
                        continue;
                    }

                    if !self.nodes.contains(&from_node.node_id) {
                        tracing::warn!(
                            "ignore slot connection because node({}) is not in runtime nodes or value nodes",
                            from_node.node_id
                        );
                        continue;
                    }

                    self.node_inputs_froms.add(
                        subflow_node_id.to_owned(),
                        runtime_handle.clone(),
                        HandleFrom::FromNodeOutput {
                            node_id: from_node.node_id.to_owned(),
                            node_output_handle: from_node.output_handle.to_owned(),
                        },
                    );

                    self.node_outputs_tos.add(
                        from_node.node_id.to_owned(),
                        from_node.output_handle.to_owned(),
                        HandleTo::ToNodeInput {
                            node_id: subflow_node_id.to_owned(),
                            node_input_handle: runtime_handle.clone(),
                        },
                    );
                }
            }

            if let Some(from_flow) = slot_input_from.from_flow {
                for flow_handle in from_flow {
                    self.flow_inputs_tos.add(
                        flow_handle.input_handle.to_owned(),
                        HandleTo::ToNodeInput {
                            node_id: subflow_node_id.to_owned(),
                            node_input_handle: runtime_handle.clone(),
                        },
                    );
                    self.node_inputs_froms.add(
                        subflow_node_id.to_owned(),
                        runtime_handle.clone(),
                        HandleFrom::FromFlowInput {
                            input_handle: flow_handle.input_handle.to_owned(),
                        },
                    );
                }
            }
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

                if let Some(from_flow) = output_from.from_flow {
                    for flow_handle in from_flow {
                        self.flow_outputs_froms.add(
                            output_from.handle.to_owned(),
                            HandleFrom::FromFlowInput {
                                input_handle: flow_handle.input_handle.to_owned(),
                            },
                        );
                        self.flow_inputs_tos.add(
                            flow_handle.input_handle.to_owned(),
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
        find_value_node: &impl Fn(&NodeId) -> Option<manifest::ValueNode>,
        flow_inputs_def: &Option<HashMap<HandleName, manifest::InputHandle>>,
    ) {
        if let Some(inputs_from) = inputs_from {
            for input_from in inputs_from {
                if let Some(from_nodes) = &input_from.from_node {
                    for from_node in from_nodes {
                        if let Some(value_node) = find_value_node(&from_node.node_id) {
                            if let Some(input) = value_node.get_handle(&from_node.output_handle) {
                                self.node_inputs_froms.add(
                                    node_id.to_owned(),
                                    input_from.handle.to_owned(),
                                    HandleFrom::FromValue {
                                        value: input.value.clone(),
                                    },
                                );
                                tracing::debug!(
                                    "value node only add node_inputs_froms has no node_outputs_tos"
                                );
                            } else {
                                tracing::warn!(
                                    "ignore node connection because value node({}) has no output handle({})",
                                    from_node.node_id,
                                    from_node.output_handle
                                );
                            }
                            continue;
                        }

                        if !self.nodes.contains(&from_node.node_id) {
                            tracing::warn!(
                                "ignore node connection because node({}) is not in runtime nodes or value nodes",
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

                if let Some(from_flow) = &input_from.from_flow {
                    for flow_handle in from_flow {
                        if flow_inputs_def
                            .as_ref()
                            .is_some_and(|def| def.contains_key(&flow_handle.input_handle))
                        {
                            self.node_inputs_froms.add(
                                node_id.to_owned(),
                                input_from.handle.to_owned(),
                                HandleFrom::FromFlowInput {
                                    input_handle: flow_handle.input_handle.to_owned(),
                                },
                            );
                            self.flow_inputs_tos.add(
                                flow_handle.input_handle.to_owned(),
                                HandleTo::ToNodeInput {
                                    node_id: node_id.to_owned(),
                                    node_input_handle: input_from.handle.to_owned(),
                                },
                            );
                        } else {
                            // TODO: slotflow.oo.yaml has no inputs def, just add it ignore inputs def currently.
                            // this is a workaround, need to be fixed later (generate inputs def in runtime)
                            tracing::warn!("node connection add some flow input({}) but not in flow inputs def", flow_handle.input_handle);
                            self.node_inputs_froms.add(
                                node_id.to_owned(),
                                input_from.handle.to_owned(),
                                HandleFrom::FromFlowInput {
                                    input_handle: flow_handle.input_handle.to_owned(),
                                },
                            );
                            self.flow_inputs_tos.add(
                                flow_handle.input_handle.to_owned(),
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
}

impl Default for ConnNodesFroms {
    fn default() -> Self {
        Self::new()
    }
}
