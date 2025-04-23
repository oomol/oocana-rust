use serde::Deserialize;

use crate::manifest::NodeInputFrom;

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct ServiceNode {
    pub service: String,
    pub node_id: NodeId,
    pub timeout: Option<u64>,
    pub inputs_from: Option<Vec<NodeInputFrom>>,
    #[serde(default = "default_concurrency")]
    pub concurrency: i32,
    #[serde(default)]
    pub ignore: bool,
}

fn default_concurrency() -> i32 {
    1
}
