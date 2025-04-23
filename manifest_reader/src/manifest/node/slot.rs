use serde::Deserialize;

use crate::manifest::{NodeInputFrom, SlotBlock};

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct SlotNode {
    pub slot: SlotNodeBlock,
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

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotNodeBlock {
    Inline(SlotBlock),
}
