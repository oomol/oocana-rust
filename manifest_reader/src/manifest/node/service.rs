use serde::Deserialize;

use crate::manifest::{InputHandle, NodeInputFrom};

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct ServiceNode {
    pub node_id: NodeId,
    pub service: String,
    pub inputs: Vec<InputHandle>,
    pub outputs: Vec<InputHandle>,
    pub concurrency: i32,
    #[serde(default)]
    pub ignore: bool,
    pub timeout: Option<u64>,
    #[serde(default)]
    pub inputs_from: Option<Vec<NodeInputFrom>>,
}
