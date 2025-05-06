use std::path::PathBuf;

use manifest_reader::{
    manifest::Node as ManifestNode, manifest::NodeId, path_finder::BlockValueType,
};

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

    pub fn package_path(&self) -> Option<PathBuf> {
        match self {
            RunningScope::Package { path, .. } => Some(path.to_path_buf()),
            RunningScope::Flow { parent, .. } => parent.as_ref().and_then(|p| p.package_path()),
            RunningScope::Slot { .. } => None,
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
