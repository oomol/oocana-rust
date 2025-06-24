use std::collections::HashSet;

use manifest_meta::{HandleFrom, Node, NodeId, SubflowBlock};

use super::node_input_values::NodeInputValues;

pub struct RunToNode {
    pub should_run_nodes: Option<HashSet<NodeId>>,
}

impl RunToNode {
    pub fn new(
        flow: &SubflowBlock,
        to_node: Option<NodeId>,
        input: Option<&NodeInputValues>,
    ) -> Self {
        Self {
            should_run_nodes: to_node.and_then(|to_node| {
                if let Some(node) = flow.nodes.get(&to_node) {
                    let mut should_run_nodes = HashSet::new();

                    calc_node_deps(node, flow, &mut should_run_nodes, &input);

                    return Some(should_run_nodes);
                }
                None
            }),
        }
    }

    pub fn has_deps_in(&self, nodes: &HashSet<NodeId>) -> bool {
        match self.should_run_nodes {
            Some(ref should_run_nodes) => !should_run_nodes.is_disjoint(nodes),
            None => false,
        }
    }

    // 返回交集
    pub fn find_intersection_nodes(&self, nodes: &HashSet<NodeId>) -> HashSet<NodeId> {
        match self.should_run_nodes {
            Some(ref should_run_nodes) => should_run_nodes.intersection(nodes).cloned().collect(),
            None => HashSet::new(),
        }
    }
}

fn calc_node_deps(
    node: &Node,
    flow: &SubflowBlock,
    should_run_nodes: &mut HashSet<NodeId>,
    node_input_values: &Option<&NodeInputValues>,
) {
    should_run_nodes.insert(node.node_id().to_owned());

    if let Some(inputs_values) = node_input_values {
        if inputs_values.is_node_fulfill(node) {
            return;
        }
    }

    if let Some(inputs) = node.inputs() {
        for (handle_name, input) in inputs.iter() {
            if node_input_values
                .as_ref()
                .map_or(false, |values| values.node_has_input(node, handle_name))
            {
                continue;
            }

            for handle_from in input.from.iter().flatten() {
                if let HandleFrom::FromNodeOutput { node_id, .. } = handle_from {
                    if !should_run_nodes.contains(node_id) {
                        if let Some(dependent_node) = flow.nodes.get(node_id) {
                            calc_node_deps(
                                dependent_node,
                                flow,
                                should_run_nodes,
                                node_input_values,
                            );
                        }
                    }
                }
            }
        }
    }
}
