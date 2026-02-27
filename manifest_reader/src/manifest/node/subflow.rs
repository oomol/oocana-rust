use serde::Deserialize;

use crate::{
    extend_node_common_field,
    manifest::{InputHandle, NodeInputFrom},
};

use super::common::{NodeId, default_concurrency, default_progress_weight};

extend_node_common_field!(SubflowNode {
    subflow: String,
    slots: Option<Vec<SlotProvider>>,
});

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotProvider {
    Task(TaskSlotProvider),
    /// this subflow is a subflow without any slots
    Subflow(SubflowSlotProvider),
    /// this slotflow is a subflow with slots
    SlotFlow(SlotFlowProvider),
}

impl SlotProvider {
    pub fn node_id(&self) -> NodeId {
        match self {
            SlotProvider::Task(slot) => slot.slot_node_id.clone(),
            SlotProvider::Subflow(slot) => slot.slot_node_id.clone(),
            SlotProvider::SlotFlow(slot_flow) => slot_flow.slot_node_id.clone(),
        }
    }

    pub fn inputs_from(&self) -> Vec<NodeInputFrom> {
        match self {
            SlotProvider::Task(_) => vec![],
            SlotProvider::Subflow(_) => vec![],
            SlotProvider::SlotFlow(slot_flow) => slot_flow.inputs_from.clone().unwrap_or_default(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SlotFlowProvider {
    pub slot_node_id: NodeId,
    pub slotflow: String,
    pub inputs_def: Option<Vec<InputHandle>>,
    pub inputs_from: Option<Vec<NodeInputFrom>>,
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
        assert!(!node.ignore);
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
    }
}
