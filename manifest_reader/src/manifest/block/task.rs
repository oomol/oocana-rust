use serde::{Deserialize, Serialize};

use super::{
    handle::{to_input_handles, to_output_handles, InputHandle, OutputHandle},
    InputHandles, OutputHandles,
};
#[derive(Deserialize, Debug, Clone)]
struct TmpTaskBlock {
    pub executor: Option<TaskBlockExecutor>,
    pub inputs_def: Option<Vec<InputHandle>>,
    pub outputs_def: Option<Vec<OutputHandle>>,
}

impl From<TmpTaskBlock> for TaskBlock {
    fn from(tmp: TmpTaskBlock) -> Self {
        TaskBlock {
            executor: tmp.executor,
            inputs_def: to_input_handles(tmp.inputs_def),
            outputs_def: to_output_handles(tmp.outputs_def),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpTaskBlock")]
pub struct TaskBlock {
    pub executor: Option<TaskBlockExecutor>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "name")]
pub enum TaskBlockExecutor {
    #[serde(rename = "nodejs")]
    NodeJS(NodeJSExecutor),
    #[serde(rename = "python")]
    Python(PythonExecutor),
    #[serde(rename = "shell")]
    Shell(ShellExecutor),
    #[serde(rename = "rust")]
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
                options.as_ref().map_or(false, |o| o.spawn)
            }
            TaskBlockExecutor::Python(PythonExecutor { options }) => {
                options.as_ref().map_or(false, |o| o.spawn)
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
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::NodeJS(e) => {
                assert!(e.options.is_some_and(|o| o.entry.is_none()));
            }
            _ => panic!("Expected NodeJSExecutor"),
        }

        let serialized = r#"{"name":"nodejs","options":{"entry":"entry1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
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
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
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
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Shell(ShellExecutor { .. }) => {}
            _ => panic!("Expected ShellExecutor"),
        }
    }

    #[test]
    fn deserialize_rust_executor() {
        let serialized = r#"{"name":"rust"}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Rust(e) => {
                assert_eq!(e.options, default_rust_spawn_options());
            }
            _ => panic!("Expected RustExecutor"),
        }
    }
}
