use serde::Deserialize;

use crate::{extend_node_common_field, manifest::NodeInputFrom};

use super::NodeId;

extend_node_common_field!(SubflowNode {
    subflow: String,
    slots: Option<Vec<SubflowNodeSlots>>,
});

fn default_concurrency() -> i32 {
    1
}

#[derive(Deserialize, Debug, Clone)]
pub struct SubflowNodeSlots {
    pub slot_node_id: NodeId,
    pub outputs_from: Vec<NodeInputFrom>,
}
