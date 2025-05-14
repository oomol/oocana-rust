mod scope;
pub use scope::RunningPackageScope;
use std::{collections::HashMap, sync::Arc};

use manifest_meta::{HandleName, NodeId};
use serde::{Deserialize, Serialize};
use utils::output::OutputValue;

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
pub struct SessionId(String);

impl SessionId {
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

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
pub struct JobId(String);

impl JobId {
    pub fn random() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

pub type BlockInputs = HashMap<HandleName, Arc<OutputValue>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockJobStackLevel {
    pub flow_job_id: JobId,
    pub flow: String,
    pub node_id: NodeId,
}

#[derive(Debug, Clone)]
pub struct BlockJobStacks(Arc<Vec<BlockJobStackLevel>>);

impl Default for BlockJobStacks {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockJobStacks {
    pub fn new() -> Self {
        Self(Arc::new(Vec::new()))
    }

    /// Stack one level
    pub fn stack(&self, flow_job_id: JobId, flow: String, node_id: NodeId) -> Self {
        let level = BlockJobStackLevel {
            flow_job_id,
            flow,
            node_id,
        };
        Self(Arc::new([self.0.to_vec(), vec![level]].concat()))
    }

    pub fn vec(&self) -> &Vec<BlockJobStackLevel> {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.len() == 0
    }
}
