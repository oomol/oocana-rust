use manifest_meta::NodeId;
use std::path::PathBuf;
use utils::calculate_short_hash;

use crate::SessionId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeScope {
    pub session_id: SessionId,
    // None means it is in workspace. Some means it is running in package.
    pub pkg_name: Option<String>,
    pub data_dir: String,
    pub pkg_root: PathBuf,
    /// if in package, this is the package path. otherwise it is the workspace path.
    pub path: PathBuf,
    pub node_id: Option<NodeId>,
    pub is_inject: bool,
    pub enable_layer: bool,
}

impl RuntimeScope {
    pub fn identifier(&self) -> String {
        let str = match &self.node_id {
            Some(node_id) => format!("{}-{}", self.path.display(), node_id),
            None => self.path.display().to_string(),
        };
        format!("{}-{}", self.session_id, calculate_short_hash(&str, 16))
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn node_id(&self) -> &Option<NodeId> {
        &self.node_id
    }

    pub fn need_layer(&self) -> bool {
        self.enable_layer
    }

    pub fn is_inject(&self) -> bool {
        self.is_inject
    }
}
