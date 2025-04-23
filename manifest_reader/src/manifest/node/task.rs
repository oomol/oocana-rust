use serde::Deserialize;
use tracing::warn;

use crate::{
    extend_node_common_field,
    manifest::{InputHandle, NodeInputFrom, TaskBlock},
    path_finder::{get_block_value_type, BlockValueType},
};

use super::common::{default_concurrency, NodeId};

extend_node_common_field!(TaskNode {
    task: TaskNodeBlock,
    inject: Option<Injection>,
    inputs_def: Option<Vec<InputHandle>>,
});

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_node() {
        let yaml = r#"
        task: example_task
        node_id: example_node
        inputs_from:
          - handle: input_handle
            value: null
        concurrency: 5
        ignore: false
        "#;

        let node: TaskNode = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(node.node_id, NodeId::from("example_node".to_owned()));
        assert_eq!(node.concurrency, 5);
        assert_eq!(node.ignore, false);
    }
}
