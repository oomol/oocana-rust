use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    derive_more::Display,
    derive_more::From,
    derive_more::FromStr,
    derive_more::Deref,
    derive_more::Constructor,
    derive_more::Into,
)]
pub struct HandleName(String);

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TempInputHandle {
    pub handle: HandleName,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub value: Option<Option<serde_json::Value>>,
    /// 如果为 true，表示 value 字段不存在时，将 value 的值从 None 更改为 Some(None)。
    #[serde(default)]
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
    pub name: Option<String>,
}

impl From<TempInputHandle> for InputHandle {
    fn from(temp: TempInputHandle) -> Self {
        let TempInputHandle {
            handle,
            value,
            json_schema,
            name,
            ..
        } = temp;
        let value = if temp.nullable {
            if value.is_none() {
                Some(None)
            } else {
                value
            }
        } else {
            value
        };
        Self {
            handle,
            value,
            json_schema,
            name,
        }
    }
}

#[derive(Serialize, Debug, Clone, Deserialize)]
#[serde(from = "TempInputHandle")]
pub struct InputHandle {
    pub handle: HandleName,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    /// 没有 value 字段，是 None;
    /// 有 value 这个 key ，但是值为 null 或者没填（yaml规范），是 Some(None);
    /// 有具体内容时，是 Some(Some(值))
    pub value: Option<Option<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
    pub name: Option<String>,
}

impl InputHandle {
    pub fn new(handle: HandleName) -> Self {
        Self {
            handle,
            value: None,
            json_schema: None,
            name: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputHandle {
    pub handle: HandleName,
    #[serde(skip_serializing_if = "Option::is_none")]
    // 真实的格式是 json schema 规范格式 + contentMediaType 字段，但是在 rust 里面这种 json + 额外字段的结构不好描述。
    // 除非把 json schema 的格式写成结构体，然后再加一个 contentMediaType 字段，但暂时没有这么大的必要性。
    pub json_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub const OOMOL_VAR_DATA: &str = "oomol/var";
pub const OOMOL_SECRET_DATA: &str = "oomol/secret";
pub const OOMOL_BIN_DATA: &str = "oomol/bin";

pub type InputHandles = HashMap<HandleName, InputHandle>;

pub fn to_input_handles(inputs: Option<Vec<InputHandle>>) -> Option<InputHandles> {
    inputs.map(|inputs| {
        inputs
            .into_iter()
            .map(|input| (input.handle.to_owned(), input))
            .collect()
    })
}

pub type OutputHandles = HashMap<HandleName, OutputHandle>;

pub fn to_output_handles(outputs: Option<Vec<OutputHandle>>) -> Option<OutputHandles> {
    outputs.map(|outputs| {
        outputs
            .into_iter()
            .map(|output| (output.handle.to_owned(), output))
            .collect()
    })
}

#[cfg(test)]
mod tests {

    #[test]
    fn serialize_content_media_type_output_handle() {
        use super::*;
        let output_handle = OutputHandle {
            handle: HandleName::new("output".to_string()),
            json_schema: Some(serde_json::json!({
                "contentMediaType": "oomol/secret"
            })),
            name: Some("var1".to_string()),
        };
        let serialized = serde_json::to_string(&output_handle).unwrap();
        assert_eq!(
            serialized,
            r#"{"handle":"output","json_schema":{"contentMediaType":"oomol/secret"},"name":"var1"}"#
        );

        let deserialized: OutputHandle = serde_json::from_str(&serialized).unwrap();
        assert_eq!(output_handle.handle, deserialized.handle);
    }

    #[test]
    fn deserialize_content_media_type_output_handle() {
        use super::*;
        let serialized = r#"{"handle":"output","json_schema":{"contentMediaType":"oomol/secret"},"name":"var1"}"#;
        let deserialized: OutputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("output".to_string()));

        match deserialized.json_schema.unwrap() {
            serde_json::Value::Object(obj) => {
                assert_eq!(obj.len(), 1);
                assert_eq!(
                    obj.get("contentMediaType"),
                    Some(&serde_json::Value::String("oomol/secret".to_string()))
                );
            }
            _ => panic!("Expected HandleJsonSchema"),
        }

        assert_eq!(deserialized.name, Some("var1".to_string()));
    }

    #[test]
    fn deserialize_null_and_missing_value_in_input_handle() {
        use super::*;
        let serialized = r#"{"handle":"input"}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, None);

        let serialized = r#"{"handle":"input","value":null}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(None));

        // yaml 中 只有 key，没有值的时候，应该被解析为 Null
        let serialized = r#"{"handle":"input","value":}"#;
        let deserialized: InputHandle = serde_yaml::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(None));
    }

    #[test]
    fn deserialize_nullable_field_modify_value_filed() {
        use super::*;
        let serialized = r#"{"handle":"input","nullable":true}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(None));

        let serialized = r#"{"handle":"input","nullable":false}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, None);

        let serialized = r#"{"handle":"input","value":null,"nullable":true}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(None));

        let serialized = r#"{"handle":"input","value":null,"nullable":false}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(None));

        let serialized = r#"{"handle":"input","value":"a","nullable":false}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(Some("a".into())));

        let serialized = r#"{"handle":"input","value":"a","nullable":true}"#;
        let deserialized: InputHandle = serde_json::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(Some("a".into())));
    }

    #[test]
    fn deserialize_yaml_input_handle() {
        use super::*;
        let serialized = r#"{"handle":"input","value":,"nullable":false}"#;
        let deserialized: InputHandle = serde_yaml::from_str(serialized).unwrap();
        assert_eq!(deserialized.handle, HandleName::new("input".to_string()));
        assert_eq!(deserialized.value, Some(None));
    }
}
