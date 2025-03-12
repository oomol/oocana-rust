use std::collections::HashSet;

use manifest_meta::{SubflowBlock, HandleFrom, Node, NodeId};

use super::node_input_values::NodeInputValues;

pub struct RunToNode {
    pub should_run_nodes: Option<HashSet<NodeId>>,
}

impl RunToNode {
    pub fn new(flow: &SubflowBlock, to_node: Option<NodeId>, input: Option<&NodeInputValues>) -> Self {
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
            Some(ref should_run_nodes) => !should_run_nodes.is_disjoint(&nodes),
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
    node: &Node, flow: &SubflowBlock, should_run_nodes: &mut HashSet<NodeId>,
    node_input_values: &Option<&NodeInputValues>,
) {
    should_run_nodes.insert(node.node_id().to_owned());

    if let Some(inputs_values) = node_input_values {
        if inputs_values.is_node_fulfill(node) {
            return;
        }
    }

    if let Some(froms) = node.from() {
        for (handle_name, handle_froms) in froms.iter() {
            // 有输入值的话，不再递归查找依赖。
            if let Some(inputs_values) = node_input_values {
                if inputs_values.node_has_input(node, handle_name.clone()) {
                    continue;
                }
            }
            for handle_from in handle_froms {
                if let HandleFrom::FromNodeOutput { node_id, .. } = handle_from {
                    if !should_run_nodes.contains(node_id) {
                        if let Some(node) = flow.nodes.get(node_id) {
                            calc_node_deps(node, flow, should_run_nodes, node_input_values);
                        }
                    }
                }
            }
        }
    }
}
