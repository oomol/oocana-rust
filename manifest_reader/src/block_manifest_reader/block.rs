use super::node::{DataSource, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
pub struct InputHandle {
    pub handle: HandleName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
    #[serde(default)]
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
    #[serde(default)]
    // 真实的格式是 json schema 规范格式 + contentMediaType 字段，但是在 rust 里面这种 json + 额外字段的结构不好描述。
    // 除非把 json schema 的格式写成结构体，然后再加一个 contentMediaType 字段，但暂时没有这么大的必要性。
    pub json_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

pub const OOMOL_VAR_DATA: &'static str = "oomol/var";
pub const OOMOL_SECRET_DATA: &'static str = "oomol/sceret";
pub const OOMOL_BIN_DATA: &'static str = "oomol/bin";

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Block {
    Task(TaskBlock),
    Flow(FlowBlock),
    Slot(SlotBlock),
}

#[derive(Deserialize, Debug, Clone)]
pub struct TaskBlock {
    pub executor: Option<TaskBlockExecutor>,
    pub entry: Option<TaskBlockEntry>,
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FlowBlock {
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub outputs_from: Option<Vec<DataSource>>,
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SlotBlock {
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TaskBlockExecutor {
    pub name: String,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TaskBlockEntry {
    pub bin: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub envs: HashMap<String, String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppletBlock {
    pub name: String,
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
}

#[cfg(test)]
mod test {
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
        let deserialized: OutputHandle = serde_json::from_str(&serialized).unwrap();
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
}
