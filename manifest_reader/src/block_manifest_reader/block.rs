use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

use super::node::{DataSource, Node};

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

#[derive(Deserialize, Debug, Clone)]
pub struct InputHandle {
    pub handle: HandleName,
    // #[serde(default)]
    // pub schema: Option<serde_json::Value>,
    // pub description: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default = "default_true")]
    pub trigger: bool,
    #[serde(default)]
    pub cache: InputHandleCache,
}

impl InputHandle {
    pub fn new(handle: HandleName) -> Self {
        Self {
            handle,
            optional: false,
            trigger: true,
            cache: InputHandleCache::Bool(false),
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum InputHandleCache {
    Bool(bool),
    InitialValue {
        initial_value: Arc<serde_json::Value>,
    },
}

impl InputHandleCache {
    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::InitialValue { .. } => true,
        }
    }

    pub fn has_value(&self) -> bool {
        match self {
            Self::Bool(_) => false,
            Self::InitialValue { .. } => true,
        }
    }
}

impl Default for InputHandleCache {
    fn default() -> Self {
        Self::Bool(false)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct OutputHandle {
    pub handle: HandleName,
    // #[serde(default)]
    // pub schema: Option<serde_json::Value>,
    // #[serde(default)]
    // pub description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Block {
    Task(TaskBlock),
    Flow(FlowBlock),
    Slot(SlotBlock),
}

#[derive(Deserialize, Debug, Clone)]
pub struct TaskBlock {
    pub entry: TaskBlockEntry,
    // #[serde(default)]
    // pub title: Option<String>,
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
    // #[serde(default)]
    // pub description: Option<String>,
    // #[serde(default)]
    // pub icon: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FlowBlock {
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub outputs_from: Option<Vec<DataSource>>,
    // #[serde(default)]
    // pub title: Option<String>,
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
    // #[serde(default)]
    // pub description: Option<String>,
    // #[serde(default)]
    // pub icon: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SlotBlock {
    // #[serde(default)]
    // pub title: Option<String>,
    #[serde(default)]
    pub inputs_def: Option<Vec<InputHandle>>,
    #[serde(default)]
    pub outputs_def: Option<Vec<OutputHandle>>,
    // #[serde(default)]
    // pub description: Option<String>,
    // #[serde(default)]
    // pub icon: Option<String>,
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
