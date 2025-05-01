use super::common::{HandlesFroms, HandlesTos};
use crate::{extend_node_common_field, Block, RunningScope, SubflowBlock, TaskBlock};
use crate::{HandleName, NodeId};
use manifest_reader::manifest::{InputDefPatch, InputHandles};
use std::collections::HashMap;
use std::sync::Arc;

extend_node_common_field!(SubflowNode {
    flow: Arc<SubflowBlock>,
    slots: Option<HashMap<NodeId, Slot>>,
    scope: RunningScope,
});

#[derive(Debug, Clone)]
pub enum Slot {
    Task(TaskSlot),
    Subflow(SubflowSlot),
}

impl Slot {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Self::Task(task_slot) => &task_slot.slot_node_id,
            Self::Subflow(subflow_slot) => &subflow_slot.slot_node_id,
        }
    }

    pub fn block(&self) -> Block {
        match self {
            Self::Task(task_slot) => Block::Task(Arc::clone(&task_slot.task)),
            Self::Subflow(subflow_slot) => Block::Flow(Arc::clone(&subflow_slot.subflow)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskSlot {
    pub slot_node_id: NodeId,
    pub task: Arc<TaskBlock>,
    pub scope: RunningScope,
}

#[derive(Debug, Clone)]
pub struct SubflowSlot {
    pub slot_node_id: NodeId,
    pub subflow: Arc<SubflowBlock>,
    pub scope: RunningScope,
}
