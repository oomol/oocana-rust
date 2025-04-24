use super::common::{HandlesFroms, HandlesTos};
use crate::{extend_node_common_field, Block, SubflowBlock, TaskBlock};
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

impl Slot {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Self::Task(task_slot) => &task_slot.slot_node_id,
            // Self::Subflow(subflow_block) => &subflow_block.node_id,
        }
    }

    pub fn block(&self) -> Block {
        match self {
            Self::Task(task_slot) => Block::Task(Arc::clone(&task_slot.task)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskSlot {
    pub slot_node_id: NodeId,
    pub task: Arc<TaskBlock>,
}
