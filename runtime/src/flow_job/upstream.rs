use std::{collections::HashSet, sync::Arc};

use tracing::warn;

use manifest_meta::{Node, NodeId, SubflowBlock};

use crate::flow_job::get_flow_cache_path;

use super::node_input_values;
use super::run_to_node::RunToNode;
use node_input_values::NodeInputValues;

pub struct UpstreamParameters {
    pub flow_block: Arc<SubflowBlock>,
    pub use_cache: bool,
    pub nodes: Option<HashSet<NodeId>>,
}

pub fn find_upstream(params: UpstreamParameters) -> (Vec<String>, Vec<String>, Vec<String>) {
    let UpstreamParameters {
        flow_block,
        use_cache,
        nodes,
    } = params;

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

/// 第一个是可以直接 run 的节点(会包含部分可以直接跑的 origin_nodes）
/// 第二个是等待的节点 nodes（不包含 origin_nodes）
/// 第三个是所有的上游 nodes（不包含 origin_nodes）
pub(crate) fn find_upstream_nodes(
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
