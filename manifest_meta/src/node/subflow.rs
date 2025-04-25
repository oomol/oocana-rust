use super::common::{HandlesFroms, HandlesTos};
use crate::{extend_node_common_field, Block, SubflowBlock, TaskBlock};
use crate::{HandleName, NodeId};
use manifest_reader::manifest::{InputDefPatch, InputHandles, Node as ManifestNode};
use std::collections::HashMap;
use std::sync::Arc;

extend_node_common_field!(SubflowNode {
    flow: Arc<SubflowBlock>,
    slots: Option<HashMap<NodeId, Slot>>,
});

#[derive(Debug, Clone)]
pub enum Slot {
    // Inline(InlineSlot),
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
}

#[derive(Debug, Clone)]
pub struct TaskSlot {
    pub slot_node_id: NodeId,
    pub task: Arc<TaskBlock>,
}

#[derive(Debug, Clone)]
pub struct SubflowSlot {
    pub slot_node_id: NodeId,
    pub subflow: Arc<SubflowBlock>,
}

#[derive(Debug, Clone)]
pub struct TmpInlineSlot {
    pub slot_node_id: NodeId,
    pub nodes: Vec<ManifestNode>,
    pub outputs_from: Vec<HandlesFroms>,
}

impl From<TmpInlineSlot> for InlineSlot {
    fn from(tmp: TmpInlineSlot) -> Self {
        let nodes = tmp
            .nodes
            .into_iter()
            .filter(|node| {
                matches!(
                    node,
                    ManifestNode::Task(_) | ManifestNode::Service(_) | ManifestNode::Value(_)
                )
            })
            .collect();

        InlineSlot {
            slot_node_id: tmp.slot_node_id,
            nodes,
            outputs_from: tmp.outputs_from,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InlineSlot {
    pub slot_node_id: NodeId,
    pub nodes: Vec<ManifestNode>, // TODO: only support task/service/value node
    pub outputs_from: Vec<HandlesFroms>,
}
