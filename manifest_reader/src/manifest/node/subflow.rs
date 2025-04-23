use serde::Deserialize;

use crate::{extend_node_common_field, manifest::NodeInputFrom};

use super::common::{default_concurrency, NodeId};

extend_node_common_field!(SubflowNode {
    subflow: String,
    slots: Option<Vec<SubflowNodeSlots>>,
});
#[derive(Deserialize, Debug, Clone)]
pub struct SubflowNodeSlots {
    pub slot_node_id: NodeId,
    pub outputs_from: Vec<NodeInputFrom>,
}
