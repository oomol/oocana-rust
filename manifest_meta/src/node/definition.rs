use std::{collections::HashMap, path::PathBuf, sync::Arc};

use manifest_reader::manifest::{InputHandles, OutputHandles};

use crate::{scope::BlockScope, Block, HandleName, NodeId, ServiceBlock, SlotBlock, TaskBlock};

use crate::extend_node_common_field;

use super::common::{HandlesTos, InputDefPatchMap, NodeInput};
use super::subflow::SubflowNode;

extend_node_common_field!(TaskNode {
    task: Arc<TaskBlock>,
    scope: BlockScope,
});

extend_node_common_field!(ServiceNode {
    block: Arc<ServiceBlock>
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

impl Node {
    pub fn description(&self) -> Option<String> {
        match self {
            Self::Task(task) => task.description.clone(),
            Self::Flow(flow) => flow.description.clone(),
            Self::Slot(slot) => slot.description.clone(),
            Self::Service(service) => service.description.clone(),
        }
    }
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

    pub fn to(&self) -> Option<&HandlesTos> {
        match self {
            Self::Task(task) => task.to.as_ref(),
            Self::Flow(flow) => flow.to.as_ref(),
            Self::Slot(slot) => slot.to.as_ref(),
            Self::Service(service) => service.to.as_ref(),
        }
    }

    pub fn inputs_def(&self) -> Option<InputHandles> {
        let inputs = self.inputs();
        let mut inputs_def = HashMap::new();
        for (handle, input) in inputs {
            inputs_def.insert(handle.clone(), input.def.clone());
        }
        if inputs_def.is_empty() {
            None
        } else {
            Some(inputs_def)
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

    pub fn inputs(&self) -> &HashMap<HandleName, NodeInput> {
        match self {
            Self::Task(task) => &task.inputs,
            Self::Flow(flow) => &flow.inputs,
            Self::Slot(slot) => &slot.inputs,
            Self::Service(service) => &service.inputs,
        }
    }

    pub fn update_inputs(&mut self, inputs: HashMap<HandleName, NodeInput>) {
        match self {
            Self::Task(task) => task.inputs = inputs,
            Self::Flow(flow) => flow.inputs = inputs,
            Self::Slot(slot) => slot.inputs = inputs,
            Self::Service(service) => service.inputs = inputs,
        }
    }

    pub fn inputs_def_patch(&self) -> Option<InputDefPatchMap> {
        let inputs = self.inputs();
        let mut patches = HashMap::new();

        for (handle, input) in inputs.iter() {
            if let Some(patch) = &input.patch {
                patches.insert(handle.clone(), patch.clone());
            }
        }
        if patches.is_empty() {
            None
        } else {
            Some(patches)
        }
    }

    pub fn has_connection(&self, handle: &HandleName) -> bool {
        self.inputs()
            .get(handle)
            .is_some_and(|input| input.from.as_ref().is_some_and(|f| !f.is_empty()))
    }

    pub fn package_path(&self) -> Option<PathBuf> {
        match self {
            Self::Task(task) => task.task.package_path.clone(),
            Self::Flow(flow) => flow.flow.package_path.clone(),
            Self::Slot(slot) => slot.slot.package_path.clone(),
            Self::Service(service) => service.block.package_path.clone(),
        }
    }

    pub fn timeout(&self) -> Option<u64> {
        match self {
            Self::Task(task) => task.timeout,
            Self::Flow(flow) => flow.timeout,
            Self::Slot(slot) => slot.timeout,
            Self::Service(service) => service.timeout,
        }
    }

    pub fn scope(&self) -> BlockScope {
        match self {
            Self::Task(task) => task.scope.clone(),
            Self::Flow(flow) => flow.scope.clone(),
            Self::Slot(_) => BlockScope::Slot {},
            Self::Service(_) => BlockScope::default(),
        }
    }
}
