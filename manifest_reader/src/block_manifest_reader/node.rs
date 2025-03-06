use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::block::{HandleName, SlotBlock, TaskBlock};

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    derive_more::Display,
    derive_more::From,
    derive_more::FromStr,
    derive_more::Deref,
    derive_more::Constructor,
    derive_more::Into,
)]
pub struct NodeId(String);

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Node {
    Task(TaskNode),
    Flow(FlowNode),
    Slot(SlotNode),
}

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Node::Task(task) => &task.node_id,
            Node::Flow(flow) => &flow.node_id,
            Node::Slot(slot) => &slot.node_id,
        }
    }
    pub fn inputs_from(&self) -> Option<&Vec<DataSource>> {
        match self {
            Node::Task(task) => task.inputs_from.as_ref(),
            Node::Flow(flow) => flow.inputs_from.as_ref(),
            Node::Slot(slot) => slot.inputs_from.as_ref(),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct TaskNode {
    pub task: TaskNodeBlock,
    pub node_id: NodeId,
    // pub title: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub inputs_from: Option<Vec<DataSource>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FlowNode {
    pub flow: String,
    pub slots: Option<Vec<FlowNodeSlotDataSource>>,
    pub node_id: NodeId,
    // pub title: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub inputs_from: Option<Vec<DataSource>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FlowNodeSlotDataSource {
    pub slot_node_id: NodeId,
    pub outputs_from: Vec<DataSource>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SlotNode {
    pub slot: SlotNodeBlock,
    pub node_id: NodeId,
    // pub title: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub inputs_from: Option<Vec<DataSource>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotNodeBlock {
    File(String),
    Inline(SlotBlock),
}

#[derive(Deserialize, Debug, Clone)]
pub struct DataSource {
    pub handle: HandleName,
    #[serde(default)]
    pub trigger: Option<bool>,
    #[serde(default)]
    pub cache: Option<DataSourceCache>,
    #[serde(default)]
    pub from_flow: Option<Vec<DataSourceFromFlow>>,
    #[serde(default)]
    pub from_node: Option<Vec<DataSourceFromNode>>,
    #[serde(default)]
    pub from_slot_node: Option<Vec<DataSourceFromSlotNode>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum DataSourceCache {
    Bool(bool),
    InitialValue(DataSourceCacheInitialValue),
}

#[derive(Deserialize, Debug, Clone)]
pub struct DataSourceCacheInitialValue {
    pub initial_value: Arc<serde_json::Value>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DataSourceFromFlow {
    pub input_handle: HandleName,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FromFlowInputHandle {
    pub input_handle: HandleName,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DataSourceFromNode {
    pub node_id: NodeId,
    pub output_handle: HandleName,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DataSourceFromSlotNode {
    pub flow_node_id: NodeId,
    pub node_id: NodeId,
    pub input_handle: HandleName,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum TaskNodeBlock {
    File(String),
    Inline(TaskBlock),
}
