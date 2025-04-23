use serde::Deserialize;

use crate::{
    extend_node_common_field,
    manifest::{NodeInputFrom, SlotBlock},
};

use super::NodeId;

extend_node_common_field!(SlotNode {
    slot: SlotNodeBlock,
});

fn default_concurrency() -> i32 {
    1
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotNodeBlock {
    Inline(SlotBlock),
}
