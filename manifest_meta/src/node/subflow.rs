use super::common::{HandlesTos, NodeInput};
use crate::{Block, BlockScope, SubflowBlock, TaskBlock, extend_node_common_field};
use crate::{HandleName, NodeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

extend_node_common_field!(SubflowNode {
    flow: Arc<RwLock<SubflowBlock>>,
    slots: Option<HashMap<NodeId, Slot>>,
    scope: BlockScope,
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

    pub fn scope(&self) -> &BlockScope {
        match self {
            Self::Task(task_slot) => &task_slot.scope,
            Self::Subflow(subflow_slot) => &subflow_slot.scope,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskSlot {
    pub slot_node_id: NodeId,
    pub task: Arc<TaskBlock>,
    pub scope: BlockScope,
}

#[derive(Debug, Clone)]
pub struct SubflowSlot {
    pub slot_node_id: NodeId,
    pub subflow: Arc<RwLock<SubflowBlock>>,
    pub scope: BlockScope,
}
