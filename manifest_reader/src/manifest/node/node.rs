use super::input_from::NodeInputFrom;
use crate::{
    manifest::{InputHandle, SlotBlock, TaskBlock},
    path_finder::{get_block_value_type, BlockValueType},
};
use serde::{Deserialize, Serialize};
use tracing::warn;

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
pub struct NodeId(String);

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Node {
    Task(TaskNode),
    Subflow(SubflowNode),
    Slot(SlotNode),
    Service(ServiceNode),
    Value(ValueNode),
}

impl Node {
    pub fn node_id(&self) -> &NodeId {
        match self {
            Node::Task(task) => &task.node_id,
            Node::Subflow(subflow) => &subflow.node_id,
            Node::Slot(slot) => &slot.node_id,
            Node::Service(service) => &service.node_id,
            Node::Value(value) => &value.node_id,
        }
    }

    pub fn concurrency(&self) -> i32 {
        match self {
            Node::Task(task) => task.concurrency,
            Node::Subflow(subflow) => subflow.concurrency,
            Node::Slot(slot) => slot.concurrency,
            Node::Service(service) => service.concurrency,
            Node::Value(value) => value.concurrency,
        }
    }
    pub fn inputs_from(&self) -> Option<&Vec<NodeInputFrom>> {
        match self {
            Node::Task(task) => task.inputs_from.as_ref(),
            Node::Subflow(subflow) => subflow.inputs_from.as_ref(),
            Node::Slot(slot) => slot.inputs_from.as_ref(),
            Node::Service(service) => service.inputs_from.as_ref(),
            Node::Value(_) => None,
        }
    }
    pub fn should_ignore(&self) -> bool {
        match self {
            Node::Task(task) => task.ignore,
            Node::Subflow(subflow) => subflow.ignore,
            Node::Slot(slot) => slot.ignore,
            Node::Service(service) => service.ignore,
            Node::Value(value) => value.ignore,
        }
    }

    pub fn should_spawn(&self) -> bool {
        match self {
            Node::Task(task) => match &task.task {
                TaskNodeBlock::File(_) => false,
                TaskNodeBlock::Inline(task) => task
                    .executor
                    .as_ref()
                    .map_or(false, |executor| executor.should_spawn()),
            },
            Node::Subflow(_) => false,
            Node::Slot(_) => false,
            Node::Service(_) => false,
            Node::Value(_) => false,
        }
    }
}

fn default_concurrency() -> i32 {
    1
}

#[derive(Debug, Clone, Deserialize)]
pub enum InjectionTarget {
    None,
    Package(String),
    Node(NodeId),
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
            tmp.script.as_ref().map(|s| {
                warn!("script will be ignored when injection target is node");
                s
            });
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

macro_rules! extend_node_common_field {
    ($name:ident { $($field:ident : $type:ty),* $(,)? }) => {
        #[derive(Deserialize, Debug, Clone)]
        pub struct $name {
            $(pub $field: $type,)*
            pub node_id: NodeId,
            pub timeout: Option<u64>,
            pub inputs_from: Option<Vec<NodeInputFrom>>,
            #[serde(default = "default_concurrency")]
            pub concurrency: i32,
            #[serde(default)]
            pub ignore: bool,
        }
    };
}

#[derive(Deserialize, Debug, Clone)]
pub struct ValueNode {
    pub node_id: NodeId,
    #[serde(default = "default_concurrency")]
    pub concurrency: i32,
    pub values: Vec<InputHandle>,
    #[serde(default)]
    pub ignore: bool,
}

extend_node_common_field!(TaskNode {
    task: TaskNodeBlock,
    timeout_seconds: Option<u64>,
    inject: Option<Injection>,
});

extend_node_common_field!(SubflowNode {
    subflow: String,
    slots: Option<Vec<SubflowNodeSlots>>,
});

extend_node_common_field!(ServiceNode { service: String });

extend_node_common_field!(SlotNode {
    slot: SlotNodeBlock,
});

#[derive(Deserialize, Debug, Clone)]
pub struct SubflowNodeSlots {
    pub slot_node_id: NodeId,
    pub outputs_from: Vec<NodeInputFrom>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SlotNodeBlock {
    Inline(SlotBlock),
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
            TaskNodeBlock::File(f) => get_block_value_type(&f),
            TaskNodeBlock::Inline(_) => BlockValueType::SelfBlock,
        }
    }
}
