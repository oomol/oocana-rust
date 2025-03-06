use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    handle::{to_input_handles, to_output_handles, InputHandle, OutputHandle},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Debug, Clone)]
pub struct TaskBlockEntry {
    pub bin: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub envs: HashMap<String, String>,
    pub cwd: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct TmpTaskBlock {
    pub executor: Option<TaskBlockExecutor>,
    pub entry: Option<TaskBlockEntry>,
    pub inputs_def: Option<Vec<InputHandle>>,
    pub outputs_def: Option<Vec<OutputHandle>>,
}

impl From<TmpTaskBlock> for TaskBlock {
    fn from(tmp: TmpTaskBlock) -> Self {
        TaskBlock {
            executor: tmp.executor,
            entry: tmp.entry,
            inputs_def: to_input_handles(tmp.inputs_def),
            outputs_def: to_output_handles(tmp.outputs_def),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpTaskBlock")]
pub struct TaskBlock {
    pub executor: Option<TaskBlockExecutor>,
    pub entry: Option<TaskBlockEntry>,
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
}

impl TaskBlockExecutor {
    pub fn name(&self) -> &str {
        match self {
            TaskBlockExecutor::NodeJS(_) => "nodejs",
            TaskBlockExecutor::Python(_) => "python",
            TaskBlockExecutor::Shell(_) => "shell",
        }
    }

    pub fn entry(&self) -> Option<&str> {
        match self {
            TaskBlockExecutor::NodeJS(NodeJSExecutor { options }) => {
                options.as_ref().and_then(|o| o.entry())
            }
            TaskBlockExecutor::Python(PythonExecutor { options }) => {
                options.as_ref().and_then(|o| o.entry())
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
                // options.as_ref().map_or(false, |o| o.spawn)
                options.as_ref().map_or(false, |o| {
                    if let ExecutorOptions::Inline(InlineExecutorOptions { spawn, .. }) = o {
                        *spawn
                    } else {
                        false
                    }
                })
            }
            TaskBlockExecutor::Python(PythonExecutor { options }) => {
                options.as_ref().map_or(false, |o| {
                    if let ExecutorOptions::Inline(InlineExecutorOptions { spawn, .. }) = o {
                        *spawn
                    } else {
                        false
                    }
                })
            }
            TaskBlockExecutor::Shell(_) => false,
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
#[serde(untagged)]
pub enum ExecutorOptions {
    // TODO: inline 文件不存在，相关代码都可以后续删除
    Inline(InlineExecutorOptions),
    File(FileExecutorOptions),
}

impl ExecutorOptions {
    pub fn is_inline(&self) -> bool {
        match self {
            ExecutorOptions::Inline(_) => true,
            _ => false,
        }
    }

    pub fn entry(&self) -> Option<&str> {
        match self {
            ExecutorOptions::File(FileExecutorOptions { entry, .. }) => entry.as_deref(),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InlineExecutorOptions {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default)]
    pub spawn: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileExecutorOptions {
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

    #[test]
    fn deserialize_task_executor() {
        use super::*;

        let serialized = r#"{"name":"nodejs","options":{}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::NodeJS(node_executor) => match node_executor.options {
                Some(ExecutorOptions::File(FileExecutorOptions { entry, .. })) => {
                    assert_eq!(entry, None);
                }
                _ => panic!("Expected FileExecutorOptions"),
            },
            _ => panic!("Expected NodeJSExecutor"),
        }

        let serialized = r#"{"name":"nodejs","options":{"entry":"entry1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::NodeJS(node_executor) => match node_executor.options {
                Some(ExecutorOptions::File(FileExecutorOptions { entry, .. })) => {
                    assert_eq!(entry, Some("entry1".to_string()));
                }
                _ => panic!("Expected FileExecutorOptions"),
            },
            _ => panic!("Expected NodeJSExecutor"),
        }

        let serialized = r#"{"name":"nodejs","options":{"source":"source1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::NodeJS(node_executor) => match node_executor.options {
                Some(ExecutorOptions::Inline(InlineExecutorOptions { source, .. })) => {
                    assert_eq!(source, "source1".to_string());
                }
                _ => panic!("Expected InlineExecutorOptions"),
            },
            _ => panic!("Expected NodeJSExecutor"),
        }

        let serialized = r#"{"name":"python","options":{"entry":"entry1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Python(PythonExecutor {
                options: Some(ExecutorOptions::File(FileExecutorOptions { entry, .. })),
            }) => {
                assert_eq!(entry, Some("entry1".to_string()));
            }
            _ => panic!("Expected PythonExecutor"),
        }

        let serialized = r#"{"name":"python","options":{"source":"source1"}}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Python(PythonExecutor {
                options: Some(ExecutorOptions::Inline(InlineExecutorOptions { source, .. })),
            }) => {
                assert_eq!(source, "source1".to_string());
            }
            _ => panic!("Expected PythonExecutor"),
        }

        let serialized = r#"{"name":"shell"}"#;
        let deserialized: TaskBlockExecutor = serde_json::from_str(&serialized).unwrap();
        match deserialized {
            TaskBlockExecutor::Shell(ShellExecutor { .. }) => {}
            _ => panic!("Expected ShellExecutor"),
        }
    }
}
