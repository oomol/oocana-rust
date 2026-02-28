use std::{collections::HashMap, sync::Arc};

use jsonschema::validate;
use serde_json::Value;

use manifest_meta::{HandleName, InputHandles};
use utils::output::OutputValue;

pub fn validate_inputs(
    inputs_def: &Option<InputHandles>,
    input_values: &HashMap<HandleName, Arc<OutputValue>>,
) -> HashMap<HandleName, String> {
    let mut error_handle = HashMap::new();

    for (handle, wrap_value) in input_values {
        if let Some(def) = inputs_def
            .as_ref()
            .and_then(|inputs_def| inputs_def.get(handle))
        {
            if let Some(ref json_schema) = def.json_schema {
                if let Err(err) = validate(json_schema, &wrap_value.value) {
                    error_handle.insert(
                        handle.clone(),
                        format!(
                            "value ({}) is not valid. validation error: {}",
                            wrap_value.value, err
                        ),
                    );
                }
            }
        }
    }

    error_handle
}

pub fn fulfill_nullable_and_default(
    input_values: &mut HashMap<String, Value>,
    inputs_def: &Option<InputHandles>,
) {
    if let Some(inputs_def) = inputs_def {
        for (handle, def) in inputs_def {
            if input_values.get(&handle.to_string()).is_none() {
                if def.value.is_some() {
                    let v: Value = def.value.clone().unwrap_or_default().unwrap_or(Value::Null);
                    input_values.insert(handle.to_string(), v);
                } else if def.nullable.unwrap_or(false) {
                    input_values.insert(handle.to_string(), Value::Null);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_inputs() {
        let mut inputs_def = InputHandles::default();
        inputs_def.insert(
            "input1".to_string().into(),
            manifest_meta::InputHandle {
                json_schema: Some(serde_json::json!({
                    "type": "string",
                })),
                ..manifest_meta::InputHandle::new("input1".to_string().into())
            },
        );

        let mut input_values = HashMap::new();
        input_values.insert(
            "input1".to_string().into(),
            Arc::new(OutputValue {
                value: serde_json::json!("valid string"),
                is_json_serializable: true,
            }),
        );

        let errors = validate_inputs(&Some(inputs_def), &input_values);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_invalid_inputs() {
        let mut inputs_def = InputHandles::default();
        inputs_def.insert(
            "input1".to_string().into(),
            manifest_meta::InputHandle {
                json_schema: Some(serde_json::json!({
                    "type": "number",
                })),
                ..manifest_meta::InputHandle::new("input1".to_string().into())
            },
        );

        let mut input_values = HashMap::new();
        input_values.insert(
            "input1".to_string().into(),
            Arc::new(OutputValue {
                value: serde_json::json!("valid string"),
                is_json_serializable: true,
            }),
        );

        let errors = validate_inputs(&Some(inputs_def), &input_values);
        assert!(!errors.is_empty());
        assert!(errors.contains_key(&HandleName::from("input1")));
    }
}
