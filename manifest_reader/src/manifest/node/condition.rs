use serde::Deserialize;

use crate::{
    extend_node_common_field,
    manifest::{block::ConditionBlock, InputHandle, NodeInputFrom},
};

use super::common::{default_concurrency, default_progress_weight, NodeId};

extend_node_common_field!(ConditionNode {
    // first input handle will be condition output value
    // maybe better keep same as other node type, and add a field to indicate which input is the condition output handle
    inputs_def: Option<Vec<InputHandle>>,
    conditions: ConditionBlock,
});
