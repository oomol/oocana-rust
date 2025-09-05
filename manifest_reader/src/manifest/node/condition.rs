use serde::Deserialize;

use crate::{
    extend_node_common_field,
    manifest::{block::ConditionBlock, InputHandle, NodeInputFrom},
};

use super::common::{default_concurrency, default_progress_weight, NodeId};

extend_node_common_field!(ConditionNode {
    inputs_def: Option<Vec<InputHandle>>,
    conditions: ConditionBlock,
});
