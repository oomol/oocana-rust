use std::sync::{Arc, RwLock};

use manifest_reader::manifest::InputHandles;

use crate::{ServiceBlock, SlotBlock, SubflowBlock, TaskBlock, condition::ConditionBlock};

#[derive(Debug, Clone)]
pub enum Block {
    Task(Arc<TaskBlock>),
    Flow(Arc<RwLock<SubflowBlock>>),
    Slot(Arc<SlotBlock>),
    Service(Arc<ServiceBlock>),
    Condition(Arc<ConditionBlock>),
}

impl Block {
    pub fn path_str(&self) -> Option<String> {
        match self {
            Block::Task(task) => task.path_str(),
            Block::Flow(flow) => Some(flow.read().unwrap().path_str.clone()),
            Block::Slot(_) => None,
            Block::Service(_) => None,
            Block::Condition(_) => None,
        }
    }

    pub fn inputs_def(&self) -> Option<InputHandles> {
        match self {
            Block::Task(task) => task.inputs_def.clone(),
            Block::Flow(flow) => flow.read().unwrap().inputs_def.clone(),
            Block::Slot(slot) => slot.inputs_def.clone(),
            Block::Service(service) => service.inputs_def.clone(),
            Block::Condition(_) => None,
        }
    }
}
