use super::common::{HandlesTos, NodeInput};
use crate::{extend_node_common_field, Block, BlockScope, SubflowBlock, TaskBlock};
use crate::{HandleName, NodeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Reference to a subflow that can be either immediately resolved or lazily loaded.
///
/// Lazy loading is used to handle circular dependencies at parse time while allowing
/// runtime to decide if the cycle is actually executed based on conditional logic.
#[derive(Debug, Clone)]
pub enum FlowReference {
    /// Fully resolved subflow loaded at parse time
    Resolved(Arc<SubflowBlock>),
    /// Deferred subflow to be loaded at runtime (used for circular references)
    Lazy {
        /// Name/path of the flow to resolve at runtime
        flow_name: String,
        /// Absolute path to the flow file (for cache lookup)
        flow_path: PathBuf,
    },
}

impl FlowReference {
    /// Returns the resolved flow if immediately available, None if lazy
    pub fn resolved(&self) -> Option<&Arc<SubflowBlock>> {
        match self {
            FlowReference::Resolved(flow) => Some(flow),
            FlowReference::Lazy { .. } => None,
        }
    }

    /// Checks if this is a lazy reference
    pub fn is_lazy(&self) -> bool {
        matches!(self, FlowReference::Lazy { .. })
    }

    /// Get the resolved flow or panic with a helpful message if lazy
    pub fn expect_resolved(&self, msg: &str) -> &Arc<SubflowBlock> {
        match self {
            FlowReference::Resolved(flow) => flow,
            FlowReference::Lazy { flow_path, .. } => {
                panic!(
                    "{msg}: Flow at {:?} is a lazy reference and must be resolved at runtime first",
                    flow_path
                )
            }
        }
    }

    /// Convert to Arc<SubflowBlock>, panicking if lazy
    pub fn into_resolved(self) -> Arc<SubflowBlock> {
        match self {
            FlowReference::Resolved(flow) => flow,
            FlowReference::Lazy { flow_path, .. } => {
                panic!(
                    "Cannot convert lazy flow reference at {:?} to Arc. \
                     Must be resolved at runtime first.",
                    flow_path
                )
            }
        }
    }
}

extend_node_common_field!(SubflowNode {
    flow: FlowReference,
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
            Self::Subflow(subflow_slot) => match &subflow_slot.subflow {
                FlowReference::Resolved(flow) => Block::Flow(Arc::clone(flow)),
                FlowReference::Lazy { flow_path, .. } => {
                    panic!(
                        "Cannot get block from lazy flow reference at {:?}. \
                         Flow must be resolved at runtime before accessing.",
                        flow_path
                    )
                }
            },
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
    pub subflow: FlowReference,
    pub scope: BlockScope,
}
