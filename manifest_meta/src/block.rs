use std::sync::Arc;

use crate::{ServiceBlock, SlotBlock, SubflowBlock, TaskBlock};

#[derive(Debug, Clone)]
pub enum Block {
    Task(Arc<TaskBlock>),
    Flow(Arc<SubflowBlock>),
    Slot(Arc<SlotBlock>),
    Service(Arc<ServiceBlock>),
}

impl Block {
    pub fn path_str(&self) -> Option<&String> {
        match self {
            Block::Task(task) => task.path_str.as_ref(),
            Block::Flow(flow) => Some(&flow.path_str),
            Block::Slot(slot) => slot.path_str.as_ref(),
            Block::Service(_) => None,
        }
    }
}
