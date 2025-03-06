use std::{collections::HashMap, sync::Arc};

use crate::{AppletBlock, FlowBlock, HandleName, InputHandle, OutputHandle, SlotBlock, TaskBlock};

#[derive(Debug, Clone)]
pub enum Block {
    Task(Arc<TaskBlock>),
    Flow(Arc<FlowBlock>),
    Slot(Arc<SlotBlock>),
    Applet(Arc<AppletBlock>),
}

impl Block {
    pub fn path_str(&self) -> Option<&String> {
        match self {
            Block::Task(task) => task.path_str.as_ref(),
            Block::Flow(flow) => Some(&flow.path_str),
            Block::Slot(slot) => slot.path_str.as_ref(),
            Block::Applet(_) => None,
        }
    }
}

pub type InputHandles = HashMap<HandleName, InputHandle>;

pub fn to_input_handles(inputs: Option<Vec<InputHandle>>) -> Option<InputHandles> {
    inputs.map(|inputs| {
        inputs
            .into_iter()
            .map(|input| (input.handle.to_owned(), input))
            .collect()
    })
}

pub type OutputHandles = HashMap<HandleName, OutputHandle>;

pub fn to_output_handles(outputs: Option<Vec<OutputHandle>>) -> Option<OutputHandles> {
    outputs.map(|outputs| {
        outputs
            .into_iter()
            .map(|output| (output.handle.to_owned(), output))
            .collect()
    })
}
