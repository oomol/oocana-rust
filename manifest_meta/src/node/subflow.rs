use super::common::{HandlesFroms, HandlesTos};
use crate::{extend_node_common_field, SubflowBlock, TaskBlock};
use crate::{HandleName, NodeId};
use manifest_reader::manifest::{InputDefPatch, InputHandles};
use std::collections::HashMap;
use std::sync::Arc;

extend_node_common_field!(SubflowNode {
    flow: Arc<SubflowBlock>,
    slots: Option<HashMap<NodeId, Slot>>,
});

#[derive(Debug, Clone)]
pub enum Slot {
    // Inline(InlineSlot),
    Task(TaskSlot),
    // Subflow(SubflowBlock),
}

#[derive(Debug, Clone)]
pub struct TaskSlot {
    pub slot_node_id: NodeId,
    pub task: Arc<TaskBlock>,
}
