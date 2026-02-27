use std::collections::HashMap;

use manifest_reader::{
    JsonValue,
    manifest::{HandleName, InputDefPatch, InputHandle, NodeId},
};

/// Represents a three-state value: not provided, explicitly null, or has value.
/// This replaces the confusing `Option<Option<JsonValue>>` pattern.
///
/// # States
/// - `NotProvided`: Value was not provided (use default or get from connection)
/// - `ExplicitNull`: Value was explicitly set to null
/// - `Value(JsonValue)`: Value has a concrete value (including JSON null)
///
/// # Note on JSON null
/// `Value(serde_json::Value::Null)` represents an explicit JSON null value,
/// which is different from `ExplicitNull`. The former is a concrete value,
/// while the latter indicates the absence of a value was explicitly specified.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ValueState {
    /// Value was not provided (use default or get from connection)
    #[default]
    NotProvided,
    /// Value was explicitly set to null (not JSON null, but absence of value)
    ExplicitNull,
    /// Value has a concrete value (can be any JSON value including null)
    Value(JsonValue),
}

impl ValueState {
    /// Create a ValueState from a JSON value
    pub fn from_json(value: JsonValue) -> Self {
        ValueState::Value(value)
    }

    /// Create an explicit null ValueState
    pub fn explicit_null() -> Self {
        ValueState::ExplicitNull
    }

    /// Check if a value was provided (either explicit null or has value)
    pub fn is_provided(&self) -> bool {
        !matches!(self, ValueState::NotProvided)
    }

    /// Check if this state has a concrete value
    pub fn has_value(&self) -> bool {
        matches!(self, ValueState::Value(_))
    }

    /// Get a reference to the value if present
    pub fn as_value(&self) -> Option<&JsonValue> {
        match self {
            ValueState::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Convert to Option<JsonValue>, losing the distinction between NotProvided and ExplicitNull
    pub fn into_option(self) -> Option<JsonValue> {
        match self {
            ValueState::Value(v) => Some(v),
            _ => None,
        }
    }

    /// Convert to JSON value, using null for non-value states
    pub fn into_json_or_null(self) -> JsonValue {
        match self {
            ValueState::Value(v) => v,
            _ => JsonValue::Null,
        }
    }
}

/// Bidirectional conversion with Option<Option<JsonValue>>
/// This is the canonical mapping:
/// - None <-> NotProvided
/// - Some(None) <-> ExplicitNull
/// - Some(Some(v)) <-> Value(v)
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

// NOTE: We intentionally do NOT implement From<Option<JsonValue>> for ValueState
// because it has ambiguous semantics: does None mean NotProvided or ExplicitNull?
// Callers should explicitly choose the correct variant.

#[derive(Debug, Clone)]
pub struct NodeInput {
    pub def: InputHandle,
    pub patch: Option<Vec<InputDefPatch>>,
    /// Value state for this input.
    /// - NotProvided: No value specified, will get from connection or use default
    /// - ExplicitNull: Explicitly set to null
    /// - Value: Has a concrete JSON value
    pub value: ValueState,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn json_string() -> JsonValue {
        JsonValue::String("test".to_string())
    }

    fn json_number() -> JsonValue {
        JsonValue::Number(42.into())
    }

    fn json_null() -> JsonValue {
        JsonValue::Null
    }

    #[test]
    fn test_not_provided_default() {
        let state = ValueState::default();
        assert!(matches!(state, ValueState::NotProvided));
        assert!(!state.is_provided());
        assert!(!state.has_value());
        assert!(state.as_value().is_none());
    }

    #[test]
    fn test_not_provided_roundtrip() {
        let from_none: ValueState = Option::<Option<JsonValue>>::None.into();
        assert!(matches!(from_none, ValueState::NotProvided));

        let back: Option<Option<JsonValue>> = ValueState::NotProvided.into();
        assert!(back.is_none());
    }

    #[test]
    fn test_explicit_null() {
        let state: ValueState = Some(None).into();
        assert!(matches!(state, ValueState::ExplicitNull));
        assert!(state.is_provided());
        assert!(!state.has_value());
        assert!(state.as_value().is_none());

        let back: Option<Option<JsonValue>> = ValueState::ExplicitNull.into();
        assert_eq!(back, Some(None));
    }

    #[test]
    fn test_value() {
        let value = json_string();
        let state: ValueState = Some(Some(value.clone())).into();
        assert!(state.is_provided());
        assert!(state.has_value());
        assert_eq!(state.as_value(), Some(&value));
        assert_eq!(state.clone().into_option(), Some(value.clone()));

        let back: Option<Option<JsonValue>> = state.into();
        assert_eq!(back, Some(Some(value)));
    }

    #[test]
    fn test_from_json_constructor() {
        let value = json_number();
        let state = ValueState::from_json(value.clone());
        assert!(matches!(state, ValueState::Value(_)));
        assert_eq!(state.as_value(), Some(&value));
    }

    #[test]
    fn test_json_null_vs_explicit_null() {
        // JSON null is a concrete value
        let json_null_state = ValueState::from_json(json_null());
        // ExplicitNull indicates absence was explicitly specified
        let explicit_null_state = ValueState::ExplicitNull;

        // JSON null HAS a value (the JSON null value)
        assert!(json_null_state.has_value());
        assert!(json_null_state.as_value().is_some());

        // ExplicitNull does NOT have a value
        assert!(!explicit_null_state.has_value());
        assert!(explicit_null_state.as_value().is_none());

        // Both are "provided"
        assert!(json_null_state.is_provided());
        assert!(explicit_null_state.is_provided());
    }

    #[test]
    fn test_into_json_or_null() {
        assert_eq!(
            ValueState::Value(json_string()).into_json_or_null(),
            json_string()
        );
        assert_eq!(ValueState::ExplicitNull.into_json_or_null(), json_null());
        assert_eq!(ValueState::NotProvided.into_json_or_null(), json_null());
    }

    #[test]
    fn test_equality() {
        assert_eq!(ValueState::NotProvided, ValueState::NotProvided);
        assert_eq!(ValueState::ExplicitNull, ValueState::ExplicitNull);
        assert_ne!(ValueState::NotProvided, ValueState::ExplicitNull);
        assert_ne!(ValueState::ExplicitNull, ValueState::Value(json_null()));
    }
}
