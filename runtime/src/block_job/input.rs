use std::{collections::HashMap, sync::Arc};

use jsonschema::validate;

use manifest_meta::{HandleName, InputHandles};
use utils::output::OutputValue;

pub fn validate_inputs(
    inputs_def: &Option<InputHandles>,
    input_values: &HashMap<HandleName, Arc<OutputValue>>,
) -> HashMap<HandleName, String> {
    let mut error_handle = HashMap::new();

    for (handle, wrap_value) in input_values {
        inputs_def
            .as_ref()
            .and_then(|inputs_def| inputs_def.get(handle))
            .map(|def| {
                if let Some(ref json_schema) = def.json_schema {
                    match validate(json_schema, &wrap_value.value) {
                        Ok(()) => {}
                        Err(err) => {
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
            });
    }

    error_handle
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
