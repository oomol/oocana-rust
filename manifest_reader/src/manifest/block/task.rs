use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::manifest::block::handle::{MiddleInputHandle, MiddleOutputHandle};

use super::{
    handle::{convert_middle_inputs, convert_middle_outputs},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
enum AdditionalObject {
    Bool(bool),
    Value(serde_json::Value),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TmpTaskBlock {
    pub description: Option<String>,
    pub executor: Arc<TaskBlockExecutor>,
    pub inputs_def: Option<Vec<MiddleInputHandle>>,
    pub outputs_def: Option<Vec<MiddleOutputHandle>>,
    #[serde(default)]
    pub additional_inputs: Option<AdditionalObject>,
    #[serde(default)]
    pub additional_outputs: Option<AdditionalObject>,
}

impl From<TmpTaskBlock> for TaskBlock {
    fn from(tmp: TmpTaskBlock) -> Self {
        Self {
            description: tmp.description,
            executor: tmp.executor,
            inputs_def: convert_middle_inputs(tmp.inputs_def),
            outputs_def: convert_middle_outputs(tmp.outputs_def),
            additional_inputs: match tmp.additional_inputs {
                Some(AdditionalObject::Bool(b)) => b,
                Some(AdditionalObject::Value(_)) => true,
                None => false,
            },
            additional_outputs: match tmp.additional_outputs {
                Some(AdditionalObject::Bool(b)) => b,
                Some(AdditionalObject::Value(_)) => true,
                None => false,
            },
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpTaskBlock")]
pub struct TaskBlock {
    pub description: Option<String>,
    pub executor: Arc<TaskBlockExecutor>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub additional_inputs: bool,
    pub additional_outputs: bool,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "name", rename_all = "lowercase")]
pub enum TaskBlockExecutor {
    NodeJS(NodeJSExecutor),
    Python(PythonExecutor),
    Shell(ShellExecutor),
    Rust(RustExecutor),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RustExecutor {
    #[serde(default = "default_rust_spawn_options")]
    pub options: SpawnOptions,
}

pub fn default_rust_spawn_options() -> SpawnOptions {
    SpawnOptions {
        bin: "cargo".to_string(),
        args: vec!["run".to_string(), "--".to_string()],
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SpawnOptions {
    pub bin: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl TaskBlockExecutor {
    pub fn name(&self) -> &str {
        match self {
            TaskBlockExecutor::NodeJS(_) => "nodejs",
            TaskBlockExecutor::Python(_) => "python",
            TaskBlockExecutor::Shell(_) => "shell",
            TaskBlockExecutor::Rust(_) => "rust",
        }
    }

    pub fn entry(&self) -> Option<&str> {
        match self {
            TaskBlockExecutor::NodeJS(NodeJSExecutor { options }) => {
                options.as_ref().and_then(|o| o.entry.as_deref())
            }
            TaskBlockExecutor::Python(PythonExecutor { options }) => {
                options.as_ref().and_then(|o| o.entry.as_deref())
            }
            _ => None,
        }
    }

    pub fn is_script(&self) -> bool {
        if let Some(entry) = self.entry() {
            // 涉及 ui 知识和其他运行逻辑。在 flow 上显示的小脚本内容，现在是真实文件，文件存储在 flow.oo.yaml 目录的 scriptlets 文件夹下。
            // 理论上用户也可以写出这个路径，目前先不考虑。
            if entry.starts_with("scriptlets/") {
                return true;
            }
        }
        false
    }

    pub fn should_spawn(&self) -> bool {
        match self {
            TaskBlockExecutor::NodeJS(NodeJSExecutor { options }) => {
                options.as_ref().is_some_and(|o| o.spawn)
            }
            TaskBlockExecutor::Python(PythonExecutor { options }) => {
                options.as_ref().is_some_and(|o| o.spawn)
            }
            TaskBlockExecutor::Shell(_) => false,
            TaskBlockExecutor::Rust(_) => false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeJSExecutor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ExecutorOptions>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PythonExecutor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ExecutorOptions>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecutorOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default)]
    pub spawn: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShellExecutor {}

#[cfg(test)]
mod test {

    use crate::manifest::{HandleName, InputHandle, OutputHandle};

    use super::*;

    #[test]
    fn serialize_task_block() {
        let tmp_task_block = TmpTaskBlock {
            description: Some("Test Task".to_string()),
            executor: Arc::new(TaskBlockExecutor::NodeJS(NodeJSExecutor {
                options: Some(ExecutorOptions {
                    entry: Some("test.js".to_string()),
                    function: None,
                    spawn: false,
                }),
            })),
            inputs_def: Some(vec![
                MiddleInputHandle::Input(InputHandle {
                    handle: HandleName::new("input1".to_string()),
                    description: Some("Input 1".to_string()),
                    json_schema: None,
                    kind: None,
                    nullable: None,
                    value: None,
                    remember: false,
                    is_additional: false,
                    _deserialize_from_cache: false,
                }),
                MiddleInputHandle::Group {
                    group: "section".to_string(),
                },
            ]),
            outputs_def: Some(vec![MiddleOutputHandle::Output(OutputHandle {
                handle: HandleName::new("output1".to_string()),
                description: Some("Output 1".to_string()),
                json_schema: None,
                kind: None,
                nullable: None,
                is_additional: false,
                _serialize_for_cache: false,
            })]),
            additional_inputs: Some(AdditionalObject::Bool(true)),
            additional_outputs: Some(AdditionalObject::Bool(true)),
        };

        let str = serde_json::to_string(&tmp_task_block).unwrap();
        println!("des: {}", str)
    }

    #[test]
    fn deserialize_task_block() {
        // Test with both additional_inputs and additional_outputs present
        {
            let str = r#"{
                "description": "Test Task",
                "executor": {
                    "name": "nodejs",
                    "options": {
                        "entry": "test.js"
                    }
                },
                "inputs_def": [{"handle": "input1", "description": "Input 1"}, {"group": "section"}],
                "outputs_def": [{"handle": "output1", "description": "Output 1"}],
                "additional_inputs": true,
                "additional_outputs": false
            }"#;

            let result = serde_json::from_str::<TaskBlock>(str);

            assert!(
                result.is_ok(),
                "Failed to deserialize TaskBlock: {:?}",
                result.err()
            );

            let block = result.unwrap();

            assert_eq!(block.description, Some("Test Task".to_string()));
            assert!(matches!(*block.executor, TaskBlockExecutor::NodeJS(_)));
            assert!(block.additional_inputs);
            assert!(!block.additional_outputs);
        }

        // Test with additional_inputs and additional_outputs as objects
        {
            let str = r#"{
                "description": "Test Task",
                "executor": {
                    "name": "nodejs",
                    "options": {
                        "entry": "test.js"
                    }
                },
                "inputs_def": [{"handle": "input1", "description": "Input 1"}, {"group": "section"}],
                "outputs_def": [{"handle": "output1", "description": "Output 1"}],
                "additional_inputs": {"some_key": "some_value"},
                "additional_outputs": {"another_key": 123}
            }"#;

            let result = serde_json::from_str::<TaskBlock>(str);

            assert!(
                result.is_ok(),
                "Failed to deserialize TaskBlock: {:?}",
                result.err()
            );

            let block = result.unwrap();

            assert_eq!(block.description, Some("Test Task".to_string()));
            assert!(matches!(*block.executor, TaskBlockExecutor::NodeJS(_)));
            assert!(block.additional_inputs);
            assert!(block.additional_outputs);
        }

        // Test with neither additional_inputs nor additional_outputs present
        {
            let str = r#"{
                "description": "Test Task",
                "executor": {
                    "name": "nodejs",
                    "options": {
                        "entry": "test.js"
                    }
                },
                "inputs_def": [{"handle": "input1", "description": "Input 1"}, {"group": "section"}],
                "outputs_def": [{"handle": "output1", "description": "Output 1"}]
            }"#;

            let result = serde_json::from_str::<TaskBlock>(str);

            assert!(
                result.is_ok(),
                "Failed to deserialize TmpTaskBlock: {:?}",
                result.err()
            );

            let block = result.unwrap();

            assert_eq!(block.description, Some("Test Task".to_string()));
            assert!(matches!(*block.executor, TaskBlockExecutor::NodeJS(_)));
            assert!(!block.additional_inputs);
            assert!(!block.additional_outputs);
        }
    }

    #[test]
    fn deserialize_nodejs_executor() {
        let serialized = r#"{"name":"nodejs","options":{}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::NodeJS(e) => {
                assert!(e.options.is_some_and(|o| o.entry.is_none()));
            }
            _ => panic!("Expected NodeJSExecutor"),
        }

        let serialized = r#"{"name":"nodejs","options":{"entry":"entry1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::NodeJS(e) => {
                assert!(e.options.as_ref().is_some_and(|o| o.entry.is_some()));
                assert_eq!(e.options.unwrap().entry.unwrap(), "entry1");
            }
            _ => panic!("Expected NodeJSExecutor"),
        }
    }

    #[test]
    fn deserialize_python_executor() {
        let serialized = r#"{"name":"python","options":{"entry":"entry1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Python(PythonExecutor {
                options: Some(ExecutorOptions { entry, .. }),
            }) => {
                assert_eq!(entry, Some("entry1".to_string()));
            }
            _ => panic!("Expected PythonExecutor"),
        }
    }

    #[test]
    fn deserialize_shell_executor() {
        let serialized = r#"{"name":"shell"}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Shell(ShellExecutor { .. }) => {}
            _ => panic!("Expected ShellExecutor"),
        }
    }

    #[test]
    fn deserialize_rust_executor() {
        let serialized = r#"{"name":"rust"}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Rust(e) => {
                assert_eq!(e.options, default_rust_spawn_options());
            }
            _ => panic!("Expected RustExecutor"),
        }
    }
}
