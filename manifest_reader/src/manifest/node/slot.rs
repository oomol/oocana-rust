use serde::Deserialize;

use super::common::{default_concurrency, default_progress_weight, NodeId};
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_slot_node() {
        let json_data = json!({
            "node_id": "test_slot_node",
            "slot": {
                "inputs_from": [],
                "outputs_from": []
            }
        });

        let slot_node: SlotNode = serde_json::from_value(json_data).unwrap();

        let slot = slot_node.slot;

        match slot {
            SlotNodeBlock::Inline(slot_block) => {
                slot_block
                    .inputs_def
                    .as_ref()
                    .map(|inputs| assert_eq!(inputs.len(), 0));

                slot_block
                    .outputs_def
                    .as_ref()
                    .map(|outputs| assert_eq!(outputs.len(), 0));
            }
        }
    }
}
