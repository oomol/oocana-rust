use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    extend_node_common_field,
    manifest::{InputHandle, InputHandles, NodeInputFrom},
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
    Inline(InlineSlot),
    Task(TaskSlot),
    /// this subflow is a subflow without any slots
    Subflow(SubflowSlot),
    /// this slotflow is a subflow with slots
    SlotFlow(SlotFlow),
}

#[derive(Deserialize, Debug, Clone)]
pub struct SlotFlow {
    pub slot_node_id: NodeId,
    pub slotflow: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TmpInlineSlot {
    pub slot_node_id: NodeId,
    pub nodes: Vec<Node>,
    pub outputs_from: Vec<NodeInputFrom>,
}

impl From<TmpInlineSlot> for InlineSlot {
    fn from(tmp: TmpInlineSlot) -> Self {
        let nodes = tmp
            .nodes
            .into_iter()
            .filter(|node| matches!(node, Node::Task(_) | Node::Service(_) | Node::Value(_)))
            .collect();

        InlineSlot {
            slot_node_id: tmp.slot_node_id,
            nodes,
            outputs_from: tmp.outputs_from,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpInlineSlot")]
pub struct InlineSlot {
    pub slot_node_id: NodeId,
    pub nodes: Vec<Node>, // TODO: more strict type
    pub outputs_from: Vec<NodeInputFrom>,
}

impl InlineSlot {
    pub fn inputs_def(&self) -> InputHandles {
        let mut inputs = HashMap::new();
        for node in &self.nodes {
            if let Some(inputs_from) = node.inputs_from() {
                for input in inputs_from {
                    if let Some(handle) = input.from_flow.as_ref() {
                        let handle = handle
                            .iter()
                            .find(|h| h.input_handle == input.handle)
                            .unwrap();
                        inputs.insert(
                            handle.input_handle.to_owned(),
                            InputHandle {
                                handle: handle.input_handle.to_owned(),
                                value: None,
                                json_schema: None,
                                name: None,
                            },
                        );
                    }
                }
            }
        }
        inputs
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SubflowSlot {
    pub slot_node_id: NodeId,
    pub subflow: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TaskSlot {
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
