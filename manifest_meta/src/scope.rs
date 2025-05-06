use std::path::{Path, PathBuf};

use manifest_reader::{
    manifest::Node as ManifestNode, manifest::NodeId, path_finder::BlockValueType,
};
use utils::calculate_short_hash;

use crate::InjectionTarget;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunningScope {
    Flow {
        node_id: Option<NodeId>,
        parent: Option<Box<RunningScope>>,
    },
    Slot {
        // slot need grandparent scope and not support injection
    },
    Package {
        path: PathBuf,
        /// for now None means the block is package block, if Some, it means the block is inject to this package
        name: Option<String>,
        node_id: Option<NodeId>,
    },
}

impl RunningScope {
    pub fn workspace(&self) -> Option<PathBuf> {
        match self {
            RunningScope::Flow { parent, .. } => parent.as_ref().and_then(|p| p.workspace()),
            RunningScope::Package { path, .. } => Some(path.clone()),
            RunningScope::Slot { .. } => None,
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

    pub fn node_id(&self) -> Option<NodeId> {
        match self {
            RunningScope::Flow { node_id, .. } => node_id.clone(),
            RunningScope::Package { node_id, .. } => node_id.clone(),
            RunningScope::Slot { .. } => None,
        }
    }

    pub fn identifier(&self) -> Option<String> {
        let str = match self {
            RunningScope::Flow { node_id, .. } => {
                node_id.as_ref().map(|node_id| format!("flow-{}", node_id))
            }
            RunningScope::Package {
                path,
                node_id,
                name: _name,
            } => match node_id {
                Some(node_id) => Some(format!("{}-{}", path.display(), node_id)),
                None => Some(format!("{}", path.display())),
            },
            RunningScope::Slot { .. } => None,
        };
        str.map(|s| calculate_short_hash(&s, 16))
    }

    pub fn target(&self) -> Option<InjectionTarget> {
        match self {
            RunningScope::Flow { .. } => None,
            RunningScope::Package { path, .. } => Some(InjectionTarget::Package(path.clone())),
            RunningScope::Slot { .. } => None,
        }
    }

    pub fn clone_with_scope_node_id(&self, scope: &Self) -> Self {
        match self {
            RunningScope::Flow { .. } => RunningScope::Flow {
                node_id: scope.node_id(),
                parent: Some(Box::new(self.clone())),
            },
            RunningScope::Package { path, name, .. } => RunningScope::Package {
                node_id: scope.node_id(),
                path: path.clone(),
                name: name.clone(),
            },
            RunningScope::Slot { .. } => RunningScope::Flow {
                node_id: scope.node_id(),
                parent: Some(Box::new(self.clone())),
            },
        }
    }
}

impl Default for RunningScope {
    fn default() -> Self {
        RunningScope::Flow {
            node_id: None,
            parent: None,
        }
    }
}

pub(crate) enum RunningTarget {
    Inherit,
    Node(NodeId),
    PackageName(String),
    PackagePath {
        path: PathBuf,
        node_id: Option<NodeId>,
    },
}

// TODO: consider Pkg block without package path
pub(crate) fn calculate_running_target(
    node: &ManifestNode,
    injection: &Option<manifest_reader::manifest::Injection>,
    package_path: &Option<PathBuf>,
    block_type: BlockValueType,
) -> RunningTarget {
    if node.should_spawn() {
        if block_type == BlockValueType::Pkg && package_path.is_some() {
            return RunningTarget::PackagePath {
                path: package_path.as_ref().unwrap().clone(),
                node_id: Some(node.node_id().clone()),
            };
        }
        return RunningTarget::Node(node.node_id().clone());
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
