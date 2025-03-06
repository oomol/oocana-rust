use std::{collections::HashMap, sync::Arc};

use crate::{
    AppletBlock, Block, FlowBlock, HandleName, InputHandles, NodeId, OutputHandles, SlotBlock,
    TaskBlock,
};

pub type HandlesFroms = HashMap<HandleName, Vec<HandleFrom>>;

pub type HandlesTos = HashMap<HandleName, Vec<HandleTo>>;

pub type NodesHandlesFroms = HashMap<NodeId, HandlesFroms>;

pub type NodesHandlesTos = HashMap<NodeId, HandlesTos>;

#[derive(Debug, Clone)]
pub struct TaskNode {
    pub task: Arc<TaskBlock>,
    pub node_id: NodeId,
    pub timeout: Option<u64>,
    pub from: Option<HandlesFroms>,
    pub to: Option<HandlesTos>,
    pub inputs_def: Option<InputHandles>,
}

#[derive(Debug, Clone)]
pub struct AppletNode {
    pub block: Arc<AppletBlock>,
    pub node_id: NodeId,
    // pub title: Option<String>,
    pub timeout: Option<u64>,
    pub from: Option<HandlesFroms>,
    pub to: Option<HandlesTos>,
    pub inputs_def: Option<InputHandles>,
}

#[derive(Debug, Clone)]
pub struct FlowNode {
    pub flow: Arc<FlowBlock>,
    pub slots_inputs_to: Option<NodesHandlesTos>,
    pub slots_outputs_from: Option<NodesHandlesFroms>,
    pub node_id: NodeId,
    // pub title: Option<String>,
    pub timeout: Option<u64>,
    pub from: Option<HandlesFroms>,
    pub to: Option<HandlesTos>,
    pub inputs_def: Option<InputHandles>,
}

#[derive(Debug, Clone)]
pub struct SlotNode {
    pub slot: Arc<SlotBlock>,
    pub node_id: NodeId,
    // pub title: Option<String>,
    pub timeout: Option<u64>,
    pub from: Option<HandlesFroms>,
    pub to: Option<HandlesTos>,
    pub inputs_def: Option<InputHandles>,
}

#[derive(Debug, Clone)]
pub enum Node {
    Task(TaskNode),
    Flow(FlowNode),
    Slot(SlotNode),
    Applet(AppletNode),
}

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Self::Task(task) => &task.node_id,
            Self::Flow(flow) => &flow.node_id,
            Self::Slot(slot) => &slot.node_id,
            Self::Applet(applet) => &applet.node_id,
        }
    }

    pub fn block(&self) -> Block {
        match self {
            Self::Task(task) => Block::Task(Arc::clone(&task.task)),
            Self::Flow(flow) => Block::Flow(Arc::clone(&flow.flow)),
            Self::Slot(slot) => Block::Slot(Arc::clone(&slot.slot)),
            Self::Applet(applet) => Block::Applet(Arc::clone(&applet.block)),
        }
    }

    pub fn from(&self) -> Option<&HandlesFroms> {
        match self {
            Self::Task(task) => task.from.as_ref(),
            Self::Flow(flow) => flow.from.as_ref(),
            Self::Slot(slot) => slot.from.as_ref(),
            Self::Applet(applet) => applet.from.as_ref(),
        }
    }

    pub fn to(&self) -> Option<&HandlesTos> {
        match self {
            Self::Task(task) => task.to.as_ref(),
            Self::Flow(flow) => flow.to.as_ref(),
            Self::Slot(slot) => slot.to.as_ref(),
            Self::Applet(applet) => applet.to.as_ref(),
        }
    }

    pub fn inputs_def(&self) -> Option<&InputHandles> {
        match self {
            Self::Task(task) => task.inputs_def.as_ref(),
            Self::Flow(flow) => flow.inputs_def.as_ref(),
            Self::Slot(slot) => slot.inputs_def.as_ref(),
            Self::Applet(applet) => applet.inputs_def.as_ref(),
        }
    }

    pub fn outputs_def(&self) -> Option<&OutputHandles> {
        match self {
            Self::Task(task) => task.task.outputs_def.as_ref(),
            Self::Flow(flow) => flow.flow.outputs_def.as_ref(),
            Self::Slot(slot) => slot.slot.outputs_def.as_ref(),
            Self::Applet(applet) => applet.block.outputs_def.as_ref(),
        }
    }

    pub fn has_from(&self, handle: &HandleName) -> bool {
        if let Some(from) = self.from() {
            if let Some(handle_froms) = from.get(handle) {
                if handle_froms.len() > 0 {
                    return true;
                }
            }
        }

        false
    }
}

#[derive(Debug, Clone)]
pub enum HandleFrom {
    FromFlowInput {
        flow_input_handle: HandleName,
    },
    FromNodeOutput {
        node_id: NodeId,
        node_output_handle: HandleName,
    },
    FromSlotInput {
        flow_node_id: NodeId,
        slot_node_id: NodeId,
        slot_input_handle: HandleName,
    },
}

#[derive(Debug, Clone)]
pub enum HandleTo {
    ToFlowOutput {
        flow_output_handle: HandleName,
    },
    ToNodeInput {
        node_id: NodeId,
        node_input_handle: HandleName,
    },
    ToSlotOutput {
        flow_node_id: NodeId,
        slot_node_id: NodeId,
        slot_output_handle: HandleName,
    },
}
