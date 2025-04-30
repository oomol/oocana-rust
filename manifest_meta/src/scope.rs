use std::path::{Path, PathBuf};

use manifest_reader::{manifest::NodeId, path_finder::BlockValueType};
use utils::calculate_short_hash;

use crate::InjectionTarget;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunningScope {
    Global {
        node_id: Option<NodeId>,
        workspace: Option<PathBuf>,
    },
    Package {
        path: PathBuf,
        /// for now None means the block is package block, if Some, it means the block is inject to this package
        name: Option<String>,
        node_id: Option<NodeId>,
    },
}

impl RunningScope {
    pub fn workspace(&self) -> PathBuf {
        match self {
            RunningScope::Global { workspace, .. } => workspace
                .clone()
                .unwrap_or_else(|| PathBuf::from("/app/workspace")),
            RunningScope::Package { path, .. } => path.clone(),
        }
    }

    pub fn package_path(&self) -> Option<&Path> {
        match self {
            RunningScope::Package { path, .. } => Some(path),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<String> {
        match self {
            RunningScope::Package { name, .. } => name.clone(),
            _ => None,
        }
    }

    pub fn identifier(&self) -> Option<String> {
        let str = match self {
            RunningScope::Global { node_id, .. } => {
                node_id.as_ref().map(|node_id| format!("{}", node_id))
            }
            RunningScope::Package {
                path,
                node_id,
                name: _name,
            } => match node_id {
                Some(node_id) => Some(format!("{}-{}", path.display(), node_id)),
                None => Some(format!("{}", path.display())),
            },
        };
        str.map(|s| calculate_short_hash(&s, 16))
    }

    pub fn target(&self) -> Option<InjectionTarget> {
        match self {
            RunningScope::Global { .. } => None,
            RunningScope::Package { path, .. } => Some(InjectionTarget::Package(path.clone())),
        }
    }
}

impl Default for RunningScope {
    fn default() -> Self {
        RunningScope::Global {
            node_id: None,
            workspace: None,
        }
    }
}

pub enum RunningTarget {
    Inherit,
    Node(NodeId),
    PackageName(String),
    PackagePath {
        path: PathBuf,
        node_id: Option<NodeId>,
    },
}

pub fn calculate_running_scope(
    node: &manifest_reader::manifest::Node,
    injection: &Option<manifest_reader::manifest::Injection>,
    package_path: &Option<PathBuf>,
    block_type: BlockValueType,
) -> RunningTarget {
    if node.should_spawn() {
        return RunningTarget::Node(node.node_id().to_owned());
    }

    // package path is some not means the block is package block
    if block_type == BlockValueType::Pkg && package_path.is_some() {
        return RunningTarget::PackagePath {
            path: package_path.as_ref().unwrap().to_owned(),
            node_id: injection.as_ref().and_then(|inj| match &inj.target {
                manifest_reader::manifest::InjectionTarget::Node(node_id) => Some(node_id.clone()),
                _ => None,
            }),
        };
    }

    match injection {
        None => RunningTarget::Inherit,
        Some(injection) => match &injection.target {
            manifest_reader::manifest::InjectionTarget::Package(pkg) => {
                RunningTarget::PackageName(pkg.to_owned())
            }
            manifest_reader::manifest::InjectionTarget::Node(node_id) => {
                RunningTarget::Node(node_id.to_owned())
            }
            _ => RunningTarget::Inherit,
        },
    }
}
