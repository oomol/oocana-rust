use serde::Deserialize;

use super::common::{default_concurrency, NodeId};
use crate::{
    extend_node_common_field,
    manifest::{NodeInputFrom, SlotBlock},
};

extend_node_common_field!(SlotNode {
    slot: SlotNodeBlock,
});

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotNodeBlock {
    Inline(SlotBlock),
}
