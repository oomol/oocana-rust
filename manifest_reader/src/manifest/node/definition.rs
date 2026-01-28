use crate::manifest::node::ConditionNode;

use super::common::{default_concurrency, NodeId};
use super::input_from::NodeInputFrom;
use super::service::ServiceNode;
use super::slot::SlotNode;
use super::subflow::SubflowNode;
use super::task::{TaskNode, TaskNodeBlock};
use super::value::ValueNode;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct FallbackNode {
    #[serde(default = "fallback_node_id")]
    pub node_id: NodeId,
}

fn fallback_node_id() -> NodeId {
    NodeId::from("fallback".to_owned())
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Node {
    Task(TaskNode),
    Subflow(SubflowNode),
    Slot(SlotNode),
    Service(ServiceNode),
    Condition(ConditionNode),
    Value(ValueNode),
    FallBack(FallbackNode),
}

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Node::Task(task) => &task.node_id,
            Node::Subflow(subflow) => &subflow.node_id,
            Node::Slot(slot) => &slot.node_id,
            Node::Service(service) => &service.node_id,
            Node::Condition(condition) => &condition.node_id,
            Node::Value(value) => &value.node_id,
            Node::FallBack(fallback) => &fallback.node_id,
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
            Node::FallBack(_fallback) => default_concurrency(),
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
            Node::FallBack(_) => None,
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
            Node::FallBack(_) => true,
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
            Node::FallBack(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml;

    #[test]
    fn test_fallback_node() {
        let yaml = r#"
        a: c
        "#;

        let node: FallbackNode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(node.node_id, NodeId::from("fallback".to_owned()));
    }

    #[test]
    fn test_nodes() {
        let yaml = r#"
        - task: example_task
          node_id: example_node
          inputs_from:
            - handle: input_handle
              value: null
          concurrency: 5
          ignore: false
        - slot: example_slot
          node_id: slot_node
        - service: example_service
          node_id: service_node
        - condition:
            handle: condition_handle
          node_id: condition_node
        - value:
            key: example_key
            value: example_value
          node_id: value_node
        - fallback:
            node_id: fallback_node
        - comment: This is a comment and should be ignored
        "#;

        let nodes: Vec<Node> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(nodes.len(), 7);
        assert_eq!(nodes[0].node_id(), &NodeId::from("example_node".to_owned()));
        assert_eq!(nodes[1].node_id(), &NodeId::from("slot_node".to_owned()));
        assert_eq!(nodes[2].node_id(), &NodeId::from("service_node".to_owned()));
        assert_eq!(
            nodes[3].node_id(),
            &NodeId::from("condition_node".to_owned())
        );
        assert_eq!(nodes[4].node_id(), &NodeId::from("value_node".to_owned()));
        assert_eq!(nodes[5].node_id(), &NodeId::from("fallback".to_owned()));
    }
}
