use super::common::{HandlesFroms, HandlesTos};
use crate::{extend_node_common_field, SubflowBlock};
use crate::{HandleName, NodeId};
use manifest_reader::manifest::{InputDefPatch, InputHandles};
use std::collections::HashMap;
use std::sync::Arc;

extend_node_common_field!(SubflowNode {
    flow: Arc<SubflowBlock>,
});
