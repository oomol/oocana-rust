use serde::Deserialize;

use crate::manifest::NodeInputFrom;

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct SubflowNode {
    pub node_id: NodeId,
    pub timeout: Option<u64>,
    pub inputs_from: Option<Vec<NodeInputFrom>>,
    #[serde(default = "default_concurrency")]
    pub concurrency: i32,
    #[serde(default)]
    pub ignore: bool,
    pub subflow: String,
    pub slots: Option<Vec<SubflowNodeSlots>>,
}

fn default_concurrency() -> i32 {
    1
}

#[derive(Deserialize, Debug, Clone)]
pub struct SubflowNodeSlots {
    pub slot_node_id: NodeId,
    pub outputs_from: Vec<NodeInputFrom>,
}
