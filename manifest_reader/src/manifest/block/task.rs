use serde::{Deserialize, Serialize};

use super::{
    handle::{to_input_handles, to_output_handles, InputHandle, OutputHandle},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
enum MiddleInputHandle {
    Input(InputHandle),
    #[allow(dead_code)]
    Group {
        group: String,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
enum MiddleOutputHandle {
    Output(OutputHandle),
    #[allow(dead_code)]
    Group {
        group: String,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TmpTaskBlock {
    pub description: Option<String>,
    pub executor: Option<TaskBlockExecutor>,
    pub inputs_def: Option<Vec<MiddleInputHandle>>,
    pub outputs_def: Option<Vec<MiddleOutputHandle>>,
    #[serde(default)]
    pub additional_inputs: bool,
    #[serde(default)]
    pub additional_outputs: bool,
}

impl From<TmpTaskBlock> for TaskBlock {
    fn from(tmp: TmpTaskBlock) -> Self {
        Self {
            description: tmp.description,
            executor: tmp.executor,
            inputs_def: to_input_handles(tmp.inputs_def.map(|v| {
                v.into_iter()
                    .filter_map(|h| match h {
                        MiddleInputHandle::Input(handle) => Some(handle),
                        MiddleInputHandle::Group { .. } => None,
                    })
                    .collect()
            })),
            outputs_def: to_output_handles(tmp.outputs_def.map(|v| {
                v.into_iter()
                    .filter_map(|h| match h {
                        MiddleOutputHandle::Output(handle) => Some(handle),
                        MiddleOutputHandle::Group { .. } => None,
                    })
                    .collect()
            })),
            additional_inputs: tmp.additional_inputs,
            additional_outputs: tmp.additional_outputs,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpTaskBlock")]
pub struct TaskBlock {
    pub description: Option<String>,
    pub executor: Option<TaskBlockExecutor>,
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

    use super::*;
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
