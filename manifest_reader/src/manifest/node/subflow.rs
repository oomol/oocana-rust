use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::{
    extend_node_common_field,
    manifest::{InputHandle, InputHandles, NodeInputFrom, OutputHandle, OutputHandles},
};

use super::{
    common::{default_concurrency, NodeId},
    Node,
};

extend_node_common_field!(SubflowNode {
    subflow: String,
    slots: Option<Vec<SlotProvider>>,
});

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotProvider {
    Inline(InlineSlotProvider),
    Task(TaskSlotProvider),
    /// this subflow is a subflow without any slots
    Subflow(SubflowSlotProvider),
    /// this slotflow is a subflow with slots
    SlotFlow(SlotFlowProvider),
}

#[derive(Deserialize, Debug, Clone)]
pub struct SlotFlowProvider {
    pub slot_node_id: NodeId,
    pub slotflow: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TmpInlineSlotProvider {
    pub slot_node_id: NodeId,
    pub nodes: Vec<Node>,
    pub outputs_from: Vec<NodeInputFrom>,
}

impl From<TmpInlineSlotProvider> for InlineSlotProvider {
    fn from(tmp: TmpInlineSlotProvider) -> Self {
        let nodes = tmp
            .nodes
            .into_iter()
            .filter(|node| matches!(node, Node::Task(_) | Node::Service(_) | Node::Value(_)))
            .collect();

        InlineSlotProvider {
            slot_node_id: tmp.slot_node_id,
            nodes,
            outputs_from: tmp.outputs_from,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpInlineSlotProvider")]
pub struct InlineSlotProvider {
    pub slot_node_id: NodeId,
    pub nodes: Vec<Node>, // TODO: more strict type
    pub outputs_from: Vec<NodeInputFrom>,
}

impl InlineSlotProvider {
    pub fn inputs_def(&self) -> InputHandles {
        let mut inputs = HashMap::new();
        for node in &self.nodes {
            if let Some(inputs_from) = node.inputs_from() {
                for input_from in inputs_from {
                    if let Some(from_flow) = input_from.from_flow.as_ref() {
                        for flow_source in from_flow {
                            inputs.insert(
                                flow_source.input_handle.to_owned(),
                                InputHandle {
                                    handle: flow_source.input_handle.to_owned(),
                                    value: None,
                                    json_schema: None,
                                    name: None,
                                    remember: false,
                                },
                            );
                        }
                    }
                }
            }
        }
        inputs
    }

    pub fn outputs_def(&self) -> OutputHandles {
        let mut outputs = HashMap::new();
        let node_ids = self
            .nodes
            .iter()
            .map(|node| node.node_id())
            .collect::<HashSet<_>>();
        for output in &self.outputs_from {
            if output.from_node.as_ref().is_some_and(|from_node| {
                from_node
                    .iter()
                    .any(|node| node_ids.contains(&node.node_id))
            }) {
                outputs.insert(
                    output.handle.to_owned(),
                    OutputHandle {
                        handle: output.handle.to_owned(),
                        json_schema: None,
                        name: None,
                    },
                );
            }
        }
        outputs
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SubflowSlotProvider {
    pub slot_node_id: NodeId,
    pub subflow: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TaskSlotProvider {
    pub slot_node_id: NodeId,
    pub task: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subflow_node() {
        let yaml = r#"
        subflow: example_subflow
        node_id: example_node
        inputs_from:
          - handle: input_handle
            value: null
        concurrency: 5
        ignore: false
        "#;

        let node: SubflowNode = serde_yaml::from_str(yaml).unwrap();

        // subflow value is not really usage value, just a placeholder
        assert_eq!(node.subflow, "example_subflow");
        assert_eq!(node.node_id, NodeId::from("example_node".to_owned()));
        assert_eq!(node.concurrency, 5);
        assert_eq!(node.ignore, false);
    }

    #[test]
    fn test_subflow_slots() {
        let yaml = r#"
        subflow: example_subflow
        node_id: example_node
        slots:
          - slot_node_id: example_task_slot
            task: example_task
          - slot_node_id: example_subflow_slot
            subflow: example_subflow_2
        "#;

        let _node: SubflowNode = serde_yaml::from_str(yaml).unwrap();

        let yaml = r#"
        subflow: example_subflow
        node_id: example_node
        slots:
          - nodes:
            - task: example_task
              node_id: example_node
              inputs_from:
                - handle: input_handle
                  value: null
              concurrency: 5
              ignore: false
            slot_node_id: node_id
            outputs_from:
              - handle: output_handle
                from_node:
                    - node_id: example_node
                      output_handle: output_handle
        "#;
        let _node: SubflowNode = serde_yaml::from_str(yaml).unwrap();
    }
}
