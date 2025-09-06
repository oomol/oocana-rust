use std::sync::Arc;

use manifest_reader::manifest::InputHandles;

use crate::{condition::ConditionBlock, ServiceBlock, SlotBlock, SubflowBlock, TaskBlock};

#[derive(Debug, Clone)]
pub enum Block {
    Task(Arc<TaskBlock>),
    Flow(Arc<SubflowBlock>),
    Slot(Arc<SlotBlock>),
    Service(Arc<ServiceBlock>),
    Condition(Arc<ConditionBlock>),
}

impl Block {
    pub fn path_str(&self) -> Option<String> {
        match self {
            Block::Task(task) => task.path_str(),
            Block::Flow(flow) => Some(flow.path_str.clone()),
            Block::Slot(_) => None,
            Block::Service(_) => None,
            Block::Condition(_) => None,
        }
    }

    pub fn inputs_def(&self) -> Option<&InputHandles> {
        match self {
            Block::Task(task) => task.inputs_def.as_ref(),
            Block::Flow(flow) => flow.inputs_def.as_ref(),
            Block::Slot(slot) => slot.inputs_def.as_ref(),
            Block::Service(service) => service.inputs_def.as_ref(),
            Block::Condition(_) => None,
        }
    }

    #[cfg(test)]
    pub fn variant_name(&self) -> &'static str {
        match self {
            Block::Task(_) => "Task Block",
            Block::Flow(_) => "Flow/Subflow Block",
            Block::Slot(_) => "Slot Block",
            Block::Service(_) => "Service Block",
            Block::Condition(_) => "Condition Block",
        }
    }
}
