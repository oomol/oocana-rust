use std::collections::HashSet;

use manifest_meta::{FlowBlock, HandleFrom, Node, NodeId};

pub struct RunToNode {
    pub should_run_nodes: Option<HashSet<NodeId>>,
}

impl RunToNode {
    pub fn new(flow: &FlowBlock, to_node: Option<NodeId>) -> Self {
        Self {
            should_run_nodes: to_node.and_then(|to_node| {
                if let Some(node) = flow.nodes.get(&to_node) {
                    let mut should_run_nodes = HashSet::new();

                    calc_node_deps(node, flow, &mut should_run_nodes);

                    return Some(should_run_nodes);
                }
                None
            }),
        }
    }

    pub fn should_run(&self, node_id: &NodeId) -> bool {
        if let Some(should_run_nodes) = &self.should_run_nodes {
            should_run_nodes.contains(node_id)
        } else {
            true
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

fn calc_node_deps(node: &Node, flow: &FlowBlock, should_run_nodes: &mut HashSet<NodeId>) {
    should_run_nodes.insert(node.node_id().to_owned());
    if let Some(froms) = node.from() {
        for handle_froms in froms.values() {
            for handle_from in handle_froms {
                if let HandleFrom::FromNodeOutput { node_id, .. } = handle_from {
                    if !should_run_nodes.contains(node_id) {
                        if let Some(node) = flow.nodes.get(node_id) {
                            calc_node_deps(node, flow, should_run_nodes);
                        }
                    }
                }
            }
        }
    }
}
