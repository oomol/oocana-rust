use std::{collections::HashMap, path::PathBuf, sync::Arc};

use manifest_reader::manifest::{InputDefPatch, InputHandles, OutputHandles};
use manifest_reader::JsonValue;

use crate::{scope::RunningScope, Block, HandleName, NodeId, ServiceBlock, SlotBlock, TaskBlock};

use crate::extend_node_common_field;

use super::common::{HandlesFroms, HandlesTos, InputDefPatchMap, NodeInput};
use super::subflow::SubflowNode;
use super::HandleFrom;

extend_node_common_field!(TaskNode {
    task: Arc<TaskBlock>,
    scope: RunningScope,
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

    pub fn inputs(&self) -> Option<&HashMap<HandleName, NodeInput>> {
        match self {
            Self::Task(task) => task.inputs.as_ref(),
            Self::Flow(flow) => flow.inputs.as_ref(),
            Self::Slot(slot) => slot.inputs.as_ref(),
            Self::Service(service) => service.inputs.as_ref(),
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

    pub fn has_only_one_value_from(&self, handle: &HandleName) -> bool {
        if let Some(from) = self.from() {
            if let Some(handle_froms) = from.get(handle) {
                if handle_froms.len() == 1 {
                    if let Some(from) = handle_froms.first() {
                        if let HandleFrom::FromValue { .. } = from {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn get_value_from(&self, handle: &HandleName) -> Option<Option<JsonValue>> {
        if let Some(from) = self.from() {
            if let Some(handle_froms) = from.get(handle) {
                if handle_froms.len() == 1 {
                    if let Some(from) = handle_froms.first() {
                        if let HandleFrom::FromValue { value } = from {
                            return value.clone();
                        }
                    }
                }
            }
        }
        None
    }

    pub fn has_connection(&self, handle: &HandleName) -> bool {
        if let Some(from) = self.from() {
            if let Some(handle_froms) = from.get(handle) {
                if handle_froms.len() == 1
                    && handle_froms
                        .first()
                        .is_some_and(|f| matches!(f, HandleFrom::FromValue { .. }))
                {
                    return false;
                } else {
                    return !handle_froms.is_empty();
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

    pub fn timeout(&self) -> Option<u64> {
        match self {
            Self::Task(task) => task.timeout,
            Self::Flow(flow) => flow.timeout,
            Self::Slot(slot) => slot.timeout,
            Self::Service(service) => service.timeout,
        }
    }

    pub fn scope(&self) -> RunningScope {
        match self {
            Self::Task(task) => task.scope.clone(),
            Self::Flow(flow) => flow.scope.clone(),
            Self::Slot(_) => RunningScope::Slot {},
            Self::Service(_) => RunningScope::default(),
        }
    }
}
