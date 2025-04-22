use serde::Deserialize;
use tracing::warn;

use crate::{
    manifest::{InputHandle, NodeInputFrom, TaskBlock},
    path_finder::{get_block_value_type, BlockValueType},
};

use super::common::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct TaskNode {
    pub task: TaskNodeBlock,
    pub node_id: NodeId,
    pub timeout: Option<u64>,
    pub inputs_from: Option<Vec<NodeInputFrom>>,
    #[serde(default = "default_concurrency")]
    pub concurrency: i32,
    #[serde(default)]
    pub ignore: bool,
    pub timeout_seconds: Option<u64>,
    pub inject: Option<Injection>,
    pub inputs_def: Option<Vec<InputHandle>>,
}

fn default_concurrency() -> i32 {
    1
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum TaskNodeBlock {
    File(String),
    Inline(TaskBlock),
}

impl TaskNodeBlock {
    pub fn entry_file(&self) -> Option<&str> {
        match self {
            TaskNodeBlock::File(_) => None,
            TaskNodeBlock::Inline(task) => {
                if let Some(executor) = &task.executor {
                    executor.entry()
                } else {
                    None
                }
            }
        }
    }

    pub fn block_type(&self) -> BlockValueType {
        match self {
            TaskNodeBlock::File(f) => get_block_value_type(f),
            TaskNodeBlock::Inline(_) => BlockValueType::SelfBlock,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TmpInjection {
    pub package: Option<String>,
    pub node_id: Option<String>,
    pub script: Option<String>,
}

impl From<TmpInjection> for Injection {
    fn from(tmp: TmpInjection) -> Self {
        if tmp.package.is_some() && tmp.node_id.is_some() {
            warn!("Both package and node_id are provided. package will be used.");
        }
        if tmp.package.is_some() {
            Injection {
                target: InjectionTarget::Package(tmp.package.unwrap()),
                script: tmp.script,
            }
        } else if tmp.node_id.is_some() {
            tmp.script
                .as_ref()
                .inspect(|_| warn!("script will be ignored when injection target is node"));
            Injection {
                target: InjectionTarget::Node(NodeId(tmp.node_id.unwrap())),
                script: None,
            }
        } else {
            warn!("Injection target is missing. script will be ignored.");
            Injection {
                target: InjectionTarget::None,
                script: tmp.script,
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "TmpInjection")]
pub struct Injection {
    pub target: InjectionTarget,
    pub script: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub enum InjectionTarget {
    None,
    Package(String),
    Node(NodeId),
}
