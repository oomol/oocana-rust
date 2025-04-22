use serde::Deserialize;

use crate::manifest::InputHandle;

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct ValueNode {
    pub node_id: NodeId,
    pub values: Vec<InputHandle>,
    #[serde(default)]
    pub ignore: bool,
}
