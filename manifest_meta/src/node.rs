use std::{collections::HashMap, path::PathBuf, sync::Arc};

use manifest_reader::manifest::{InputDefPatch, InputHandles, OutputHandles};

use crate::{Block, HandleName, NodeId, ServiceBlock, SlotBlock, SubflowBlock, TaskBlock};

pub type HandlesFroms = HashMap<HandleName, Vec<HandleFrom>>;

pub type HandlesTos = HashMap<HandleName, Vec<HandleTo>>;

pub type NodesHandlesFroms = HashMap<NodeId, HandlesFroms>;

pub type NodesHandlesTos = HashMap<NodeId, HandlesTos>;

macro_rules! extend_node_common_field {
    ($name:ident { $($field:ident : $type:ty),* $(,)? }) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            $(pub $field: $type,)*
            pub node_id: NodeId,
            pub timeout: Option<u64>,
            pub from: Option<HandlesFroms>,
            pub to: Option<HandlesTos>,
            pub inputs_def: Option<InputHandles>,
            pub concurrency: i32,
            pub inputs_def_patch: Option<HashMap<HandleName, Vec<InputDefPatch>>>,
        }
    };
}

extend_node_common_field!(TaskNode {
    task: Arc<TaskBlock>,
    timeout_seconds: Option<u64>,
});

extend_node_common_field!(ServiceNode {
    block: Arc<ServiceBlock>
});

extend_node_common_field!(SubflowNode {
    flow: Arc<SubflowBlock>,
    slots_inputs_to: Option<NodesHandlesTos>,
    slots_outputs_from: Option<NodesHandlesFroms>,
});

extend_node_common_field!(SlotNode {
    slot: Arc<SlotBlock>,
});

#[derive(Debug, Clone)]
pub enum Node {
    Task(TaskNode),
    Flow(SubflowNode),
    Slot(SlotNode),
    Service(ServiceNode),
}

pub type InputDefPatchMap = HashMap<HandleName, Vec<InputDefPatch>>;

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Self::Task(task) => &task.node_id,
            Self::Flow(flow) => &flow.node_id,
            Self::Slot(slot) => &slot.node_id,
            Self::Service(service) => &service.node_id,
        }
    }

    pub fn concurrency(&self) -> i32 {
        match self {
            Self::Task(task) => task.concurrency,
            Self::Flow(flow) => flow.concurrency,
            Self::Slot(slot) => slot.concurrency,
            Self::Service(service) => service.concurrency,
        }
    }

    pub fn block(&self) -> Block {
        match self {
            Self::Task(task) => Block::Task(Arc::clone(&task.task)),
            Self::Flow(flow) => Block::Flow(Arc::clone(&flow.flow)),
            Self::Slot(slot) => Block::Slot(Arc::clone(&slot.slot)),
            Self::Service(service) => Block::Service(Arc::clone(&service.block)),
        }
    }

    pub fn from(&self) -> Option<&HandlesFroms> {
        match self {
            Self::Task(task) => task.from.as_ref(),
            Self::Flow(flow) => flow.from.as_ref(),
            Self::Slot(slot) => slot.from.as_ref(),
            Self::Service(service) => service.from.as_ref(),
        }
    }

    pub fn to(&self) -> Option<&HandlesTos> {
        match self {
            Self::Task(task) => task.to.as_ref(),
            Self::Flow(flow) => flow.to.as_ref(),
            Self::Slot(slot) => slot.to.as_ref(),
            Self::Service(service) => service.to.as_ref(),
        }
    }

    pub fn inputs_def(&self) -> Option<&InputHandles> {
        match self {
            Self::Task(task) => task.inputs_def.as_ref(),
            Self::Flow(flow) => flow.inputs_def.as_ref(),
            Self::Slot(slot) => slot.inputs_def.as_ref(),
            Self::Service(service) => service.inputs_def.as_ref(),
        }
    }

    pub fn outputs_def(&self) -> Option<&OutputHandles> {
        match self {
            Self::Task(task) => task.task.outputs_def.as_ref(),
            Self::Flow(flow) => flow.flow.outputs_def.as_ref(),
            Self::Slot(slot) => slot.slot.outputs_def.as_ref(),
            Self::Service(service) => service.block.outputs_def.as_ref(),
        }
    }

    pub fn inputs_def_patch(&self) -> Option<&InputDefPatchMap> {
        match self {
            Self::Task(task) => task.inputs_def_patch.as_ref(),
            Self::Flow(flow) => flow.inputs_def_patch.as_ref(),
            Self::Slot(slot) => slot.inputs_def_patch.as_ref(),
            Self::Service(service) => service.inputs_def_patch.as_ref(),
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

    pub fn package_path(&self) -> Option<PathBuf> {
        match self {
            Self::Task(task) => task.task.package_path.clone(),
            Self::Flow(flow) => flow.flow.package_path.clone(),
            Self::Slot(slot) => slot.slot.package_path.clone(),
            Self::Service(service) => service.block.package_path.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum HandleFrom {
    FromFlowInput {
        input_handle: HandleName,
    },
    FromNodeOutput {
        node_id: NodeId,
        node_output_handle: HandleName,
    },
    FromSlotInput {
        subflow_node_id: NodeId,
        slot_node_id: NodeId,
        slot_input_handle: HandleName,
    },
}

#[derive(Debug, Clone)]
pub enum HandleTo {
    ToFlowOutput {
        output_handle: HandleName,
    },
    ToNodeInput {
        node_id: NodeId,
        node_input_handle: HandleName,
    },
    ToSlotOutput {
        subflow_node_id: NodeId,
        slot_node_id: NodeId,
        slot_output_handle: HandleName,
    },
}
