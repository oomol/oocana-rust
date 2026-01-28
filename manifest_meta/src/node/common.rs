use std::collections::HashMap;

use manifest_reader::{
    manifest::{HandleName, InputDefPatch, InputHandle, NodeId},
    JsonValue,
};

/// Represents a three-state value: not provided, explicitly null, or has value.
/// This replaces the confusing `Option<Option<JsonValue>>` pattern.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ValueState {
    /// Value was not provided (use default or get from connection)
    #[default]
    NotProvided,
    /// Value was explicitly set to null
    ExplicitNull,
    /// Value has a concrete value
    Value(JsonValue),
}

impl ValueState {
    pub fn is_provided(&self) -> bool {
        !matches!(self, ValueState::NotProvided)
    }

    pub fn has_value(&self) -> bool {
        matches!(self, ValueState::Value(_))
    }

    pub fn as_value(&self) -> Option<&JsonValue> {
        match self {
            ValueState::Value(v) => Some(v),
            _ => None,
        }
    }

    pub fn into_option(self) -> Option<JsonValue> {
        match self {
            ValueState::Value(v) => Some(v),
            _ => None,
        }
    }
}

impl From<Option<Option<JsonValue>>> for ValueState {
    fn from(v: Option<Option<JsonValue>>) -> Self {
        match v {
            None => ValueState::NotProvided,
            Some(None) => ValueState::ExplicitNull,
            Some(Some(v)) => ValueState::Value(v),
        }
    }
}

impl From<ValueState> for Option<Option<JsonValue>> {
    fn from(v: ValueState) -> Self {
        match v {
            ValueState::NotProvided => None,
            ValueState::ExplicitNull => Some(None),
            ValueState::Value(v) => Some(Some(v)),
        }
    }
}

impl From<Option<JsonValue>> for ValueState {
    fn from(v: Option<JsonValue>) -> Self {
        match v {
            None => ValueState::ExplicitNull,
            Some(v) => ValueState::Value(v),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeInput {
    pub def: InputHandle,
    pub patch: Option<Vec<InputDefPatch>>,
    // generate from node's from.value or from value_node
    pub value: Option<Option<JsonValue>>,
    // generate from node's from.flow_input or from node's from.node_output
    pub sources: Option<Vec<HandleSource>>,
    pub serialize_for_cache: bool,
}

#[macro_export(local_inner_macros)]
macro_rules! extend_node_common_field {
    ($name:ident { $($field:ident : $type:ty),* $(,)? }) => {
        #[derive(Debug, Clone)]
        pub struct $name {
            $(pub $field: $type,)*
            pub node_id: NodeId,
            pub description: Option<String>,
            pub timeout: Option<u64>,
            pub to: Option<HandlesTos>,
            pub inputs: HashMap<HandleName, NodeInput>,
            pub concurrency: i32,
            pub progress_weight: f32,
        }
    };
}

pub type HandlesFroms = HashMap<HandleName, Vec<HandleFrom>>;

pub type HandlesTos = HashMap<HandleName, Vec<HandleTo>>;

pub type NodesHandlesTos = HashMap<NodeId, HandlesTos>;

pub type InputDefPatchMap = HashMap<HandleName, Vec<InputDefPatch>>;

#[derive(Debug, Clone)]
pub enum HandleSource {
    FlowInput {
        input_handle: HandleName,
    },
    NodeOutput {
        node_id: NodeId,
        output_handle: HandleName,
    },
}

#[derive(Debug, Clone)]
pub enum HandleFrom {
    FromFlowInput {
        input_handle: HandleName,
    },
    FromNodeOutput {
        node_id: NodeId,
        output_handle: HandleName,
    },
    FromValue {
        value: ValueState,
    },
}

#[derive(Debug, Clone)]
pub enum HandleTo {
    ToFlowOutput {
        output_handle: HandleName,
    },
    ToNodeInput {
        node_id: NodeId,
        input_handle: HandleName,
    },
}
