use std::path::{Path, PathBuf};

use manifest_reader::manifest::NodeId;

use crate::InjectionTarget;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunningScope {
    Global {
        node_id: Option<NodeId>,
    },
    Package {
        path: PathBuf,
        name: String,
        // node_id: Option<NodeId>,
    },
}

impl RunningScope {
    pub fn package_path(&self) -> Option<&Path> {
        match self {
            RunningScope::Package { path, .. } => Some(&path),
            _ => None,
        }
    }

    pub fn identifier(&self) -> Option<String> {
        match self {
            RunningScope::Global { node_id } => {
                if let Some(node_id) = node_id {
                    Some(format!("global-{}", node_id))
                } else {
                    None
                }
            }
            RunningScope::Package { path, .. } => {
                Some(format!("package-{}", path.to_string_lossy().to_string()))
            }
        }
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
        RunningScope::Global { node_id: None }
    }
}

pub enum RunningTarget {
    Global,
    Node(NodeId),
    PackageName(String),
    PackagePath(PathBuf),
}

pub fn calculate_running_scope(
    node: &manifest_reader::manifest::Node,
    injection: &Option<manifest_reader::manifest::Injection>,
    package_path: &Option<PathBuf>,
) -> RunningTarget {
    if node.should_spawn() {
        return RunningTarget::Node(node.node_id().to_owned());
    }

    if let Some(pkg_path) = package_path {
        return RunningTarget::PackagePath(pkg_path.to_owned());
    }

    match injection {
        None => RunningTarget::Global,
        Some(injection) => match &injection.target {
            manifest_reader::manifest::InjectionTarget::Package(pkg) => {
                RunningTarget::PackageName(pkg.to_owned())
            }
            manifest_reader::manifest::InjectionTarget::Node(node_id) => {
                RunningTarget::Node(node_id.to_owned())
            }
            _ => RunningTarget::Global,
        },
    }
}
