use std::collections::HashMap;

use manifest_reader::{
    manifest::{HandleName, InputDefPatch, NodeId},
    JsonValue,
};

#[macro_export(local_inner_macros)]
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

pub type HandlesFroms = HashMap<HandleName, Vec<HandleFrom>>;

pub type HandlesTos = HashMap<HandleName, Vec<HandleTo>>;

pub type NodesHandlesFroms = HashMap<NodeId, HandlesFroms>;

pub type NodesHandlesTos = HashMap<NodeId, HandlesTos>;

pub type InputDefPatchMap = HashMap<HandleName, Vec<InputDefPatch>>;

#[derive(Debug, Clone)]
pub enum HandleFrom {
    FromFlowInput {
        input_handle: HandleName,
    },
    FromNodeOutput {
        node_id: NodeId,
        node_output_handle: HandleName,
    },
    FromValue {
        value: Option<Option<JsonValue>>,
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
    ToSlotInput {
        node_id: NodeId,
        slot_node_id: NodeId,
        slot_input_handle: HandleName,
    },
}
