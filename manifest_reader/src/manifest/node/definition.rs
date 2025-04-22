use super::common::NodeId;
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
    Value(ValueNode),
}

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Node::Task(task) => &task.node_id,
            Node::Subflow(subflow) => &subflow.node_id,
            Node::Slot(slot) => &slot.node_id,
            Node::Service(service) => &service.node_id,
            Node::Value(value) => &value.node_id,
        }
    }

    pub fn concurrency(&self) -> i32 {
        match self {
            Node::Task(task) => task.concurrency,
            Node::Subflow(subflow) => subflow.concurrency,
            Node::Slot(slot) => slot.concurrency,
            Node::Service(service) => service.concurrency,
            Node::Value(_value) => 1,
        }
    }
    pub fn inputs_from(&self) -> Option<&Vec<NodeInputFrom>> {
        match self {
            Node::Task(task) => task.inputs_from.as_ref(),
            Node::Subflow(subflow) => subflow.inputs_from.as_ref(),
            Node::Slot(slot) => slot.inputs_from.as_ref(),
            Node::Service(service) => service.inputs_from.as_ref(),
            Node::Value(_) => None,
        }
    }
    pub fn should_ignore(&self) -> bool {
        match self {
            Node::Task(task) => task.ignore,
            Node::Subflow(subflow) => subflow.ignore,
            Node::Slot(slot) => slot.ignore,
            Node::Service(service) => service.ignore,
            Node::Value(value) => value.ignore,
        }
    }

    pub fn should_spawn(&self) -> bool {
        match self {
            Node::Task(task) => match &task.task {
                TaskNodeBlock::File(_) => false,
                TaskNodeBlock::Inline(task) => task
                    .executor
                    .as_ref()
                    .is_some_and(|executor| executor.should_spawn()),
            },
            Node::Subflow(_) => false,
            Node::Slot(_) => false,
            Node::Service(_) => false,
            Node::Value(_) => false,
        }
    }
}
