use super::common::NodeId;
use crate::manifest::{HandleName, OOMOL_SECRET_DATA};
use serde::{Deserialize, Serialize};

/// tmp struct

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
struct TmpInputDefSchema {
    #[serde(rename = "contentMediaType")]
    pub content_media_type: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct TmpInputDefPatch {
    pub path: Option<PatchPath>,
    pub schema: Option<TmpInputDefSchema>,
}

#[derive(Deserialize, Debug, Clone)]
struct TmpNodeInputFrom {
    pub handle: HandleName,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub value: Option<Option<serde_json::Value>>,
    pub schema_overrides: Option<Vec<TmpInputDefPatch>>,
    pub from_flow: Option<Vec<FlowHandleFrom>>,
    pub from_node: Option<Vec<NodeHandleFrom>>,
}

/// PatchSchema

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct PatchSchema {
    #[serde(rename = "contentMediaType")]
    pub content_media_type: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct InputDefPatch {
    pub path: Option<PatchPath>,
    pub schema: PatchSchema,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum PathElement {
    String(String),
    Index(usize),
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum PatchPath {
    String(String),
    Array(Vec<PathElement>),
    Index(usize),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpNodeInputFrom")]
pub struct NodeInputFrom {
    pub handle: HandleName,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::double_option"
    )]
    pub value: Option<Option<serde_json::Value>>,

    pub schema_overrides: Option<Vec<InputDefPatch>>,

    pub from_flow: Option<Vec<FlowHandleFrom>>,
    pub from_node: Option<Vec<NodeHandleFrom>>,
}

impl From<TmpNodeInputFrom> for NodeInputFrom {
    fn from(data: TmpNodeInputFrom) -> Self {
        // 过滤掉不是 oomol/secret 的 schema_overrides，暂时用不到。
        let used_schema_overrides: Vec<InputDefPatch> = data
            .schema_overrides
            .unwrap_or_default()
            .into_iter()
            .filter_map(|schema_override| {
                schema_override.schema.and_then(|schema| {
                    if schema.content_media_type.as_deref() == Some(OOMOL_SECRET_DATA) {
                        Some(InputDefPatch {
                            path: schema_override.path,
                            schema: PatchSchema {
                                content_media_type: schema.content_media_type.unwrap(),
                            },
                        })
                    } else {
                        None
                    }
                })
            })
            .collect();

        let schema_overrides = if used_schema_overrides.is_empty() {
            None
        } else {
            tracing::debug!("used_schema_overrides: {:?}", used_schema_overrides);
            Some(used_schema_overrides)
        };

        NodeInputFrom {
            handle: data.handle,
            value: data.value,
            schema_overrides,
            from_flow: data.from_flow,
            from_node: data.from_node,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct FlowHandleFrom {
    pub input_handle: HandleName,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NodeHandleFrom {
    pub node_id: NodeId,
    pub output_handle: HandleName,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml;

    #[test]
    fn test_null() {
        let data = serde_yaml::from_str::<NodeInputFrom>(
            r#"{"handle":"input","value":,"nullable":false}"#,
        )
        .unwrap();
        assert_eq!(data.value, Some(None));
    }

    #[test]
    fn test_input_from_deserialize() {
        let data = serde_yaml::from_str::<NodeInputFrom>(
            r#"
            handle: test
            value: "test"
            schema_overrides:
            - path: test
              schema:
                contentMediaType: "oomol/secret"
                type: object
            - path: [a, 1]
              schema:
                contentMediaType: "oomol/secret"
                type: object
            - path: 1
              schema:
                contentMediaType: "oomol/secret"
                type: object
            - path: [a, b, c]
              schema:
                type: object
            "#,
        )
        .unwrap();

        assert_eq!(data.handle, HandleName::from("test".to_string()));
        assert_eq!(
            data.value,
            Some(Some(serde_json::Value::String("test".to_string())))
        );

        assert_eq!(
            data.schema_overrides,
            Some(vec![
                InputDefPatch {
                    path: Some(PatchPath::String("test".to_string())),
                    schema: PatchSchema {
                        content_media_type: OOMOL_SECRET_DATA.to_string()
                    },
                },
                InputDefPatch {
                    path: Some(PatchPath::Array(vec![
                        PathElement::String("a".to_string()),
                        PathElement::Index(1)
                    ])),
                    schema: PatchSchema {
                        content_media_type: OOMOL_SECRET_DATA.to_string()
                    },
                },
                InputDefPatch {
                    path: Some(PatchPath::Index(1)),
                    schema: PatchSchema {
                        content_media_type: OOMOL_SECRET_DATA.to_string()
                    },
                }
            ])
        )
    }
}
