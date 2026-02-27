use crate::manifest::node::ConditionNode;

use super::common::{NodeId, default_concurrency};
use super::input_from::NodeInputFrom;
use super::service::ServiceNode;
use super::slot::SlotNode;
use super::subflow::SubflowNode;
use super::task::{TaskNode, TaskNodeBlock};
use super::value::ValueNode;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Node {
    Task(TaskNode),
    Subflow(SubflowNode),
    Slot(SlotNode),
    Service(ServiceNode),
    Condition(ConditionNode),
    Value(ValueNode),
}

// IMPORTANT: The order of variants in this untagged enum matters for serde deserialization.
// Serde tries variants in declaration order and uses the first one that matches.
//
// Current order rationale:
// 1. Task - has unique `task` field (String or inline TaskBlock)
// 2. Subflow - has unique `subflow` field
// 3. Slot - has unique `slot` field
// 4. Service - has unique `service` field
// 5. Condition - has unique `conditions` field
// 6. Value - has unique `values` field (most generic, should be last)
//
// Each node type has a distinguishing required field, so order shouldn't matter
// in practice. However, tests below verify this assumption.

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Node::Task(task) => &task.node_id,
            Node::Subflow(subflow) => &subflow.node_id,
            Node::Slot(slot) => &slot.node_id,
            Node::Service(service) => &service.node_id,
            Node::Condition(condition) => &condition.node_id,
            Node::Value(value) => &value.node_id,
        }
    }

    pub fn concurrency(&self) -> i32 {
        match self {
            Node::Task(task) => task.concurrency,
            Node::Subflow(subflow) => subflow.concurrency,
            Node::Slot(slot) => slot.concurrency,
            Node::Service(service) => service.concurrency,
            Node::Condition(condition) => condition.concurrency,
            Node::Value(_value) => default_concurrency(),
        }
    }
    pub fn inputs_from(&self) -> Option<&Vec<NodeInputFrom>> {
        match self {
            Node::Task(task) => task.inputs_from.as_ref(),
            Node::Subflow(subflow) => subflow.inputs_from.as_ref(),
            Node::Slot(slot) => slot.inputs_from.as_ref(),
            Node::Service(service) => service.inputs_from.as_ref(),
            Node::Condition(condition) => condition.inputs_from.as_ref(),
            Node::Value(_) => None,
        }
    }
    pub fn should_ignore(&self) -> bool {
        match self {
            Node::Task(task) => task.ignore,
            Node::Subflow(subflow) => subflow.ignore,
            Node::Slot(slot) => slot.ignore,
            Node::Service(service) => service.ignore,
            Node::Condition(condition) => condition.ignore,
            Node::Value(value) => value.ignore,
        }
    }

    pub fn should_spawn(&self) -> bool {
        match self {
            Node::Task(task) => match &task.task {
                TaskNodeBlock::File(_) => false,
                TaskNodeBlock::Inline(task) => task.executor.should_spawn(),
            },
            Node::Subflow(_) => false,
            Node::Slot(_) => false,
            Node::Service(_) => false,
            Node::Condition(_) => false,
            Node::Value(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests verify that the untagged enum deserializes to the correct variant.
    // If any of these tests fail, it likely means the enum variant order needs adjustment.

    #[test]
    fn node_deserializes_to_task_when_has_task_field() {
        let yaml = r#"
            node_id: test-task
            task: some_task_block
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Task(_)),
            "Expected Task variant, got {:?}",
            node
        );
    }

    #[test]
    fn node_deserializes_to_subflow_when_has_subflow_field() {
        let yaml = r#"
            node_id: test-subflow
            subflow: some_subflow
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Subflow(_)),
            "Expected Subflow variant, got {:?}",
            node
        );
    }

    #[test]
    fn node_deserializes_to_slot_when_has_slot_field() {
        let yaml = r#"
            node_id: test-slot
            slot:
              inputs_from: []
              outputs_from: []
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Slot(_)),
            "Expected Slot variant, got {:?}",
            node
        );
    }

    #[test]
    fn node_deserializes_to_service_when_has_service_field() {
        let yaml = r#"
            node_id: test-service
            service: some_service
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Service(_)),
            "Expected Service variant, got {:?}",
            node
        );
    }

    #[test]
    fn node_deserializes_to_condition_when_has_conditions_field() {
        let yaml = r#"
            node_id: test-condition
            conditions:
              cases:
                - handle: case1
                  expressions:
                    - input_handle: input1
                      operator: "=="
                      value: "expected"
              default:
                handle: default_case
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Condition(_)),
            "Expected Condition variant, got {:?}",
            node
        );
    }

    #[test]
    fn node_deserializes_to_value_when_has_values_field() {
        let yaml = r#"
            node_id: test-value
            values:
              - handle: value1
                value: 42
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Value(_)),
            "Expected Value variant, got {:?}",
            node
        );
    }

    #[test]
    fn node_deserializes_task_with_inline_block() {
        // Inline TaskBlock to verify it doesn't accidentally match other variants
        let yaml = r#"
            node_id: test-inline-task
            task:
              executor:
                name: python
                entry: main.py
        "#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        assert!(
            matches!(node, Node::Task(_)),
            "Expected Task variant with inline block, got {:?}",
            node
        );
    }

    #[test]
    fn node_vec_deserializes_mixed_types() {
        // Verify a flow with multiple node types deserializes correctly
        let yaml = r#"
            - node_id: task-1
              task: block_a
            - node_id: subflow-1
              subflow: flow_b
            - node_id: service-1
              service: svc_c
            - node_id: value-1
              values:
                - handle: v1
                  value: 123
        "#;
        let nodes: Vec<Node> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(nodes.len(), 4);
        assert!(matches!(nodes[0], Node::Task(_)));
        assert!(matches!(nodes[1], Node::Subflow(_)));
        assert!(matches!(nodes[2], Node::Service(_)));
        assert!(matches!(nodes[3], Node::Value(_)));
    }
}
