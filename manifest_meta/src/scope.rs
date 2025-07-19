use std::path::PathBuf;

use manifest_reader::{
    manifest::Node as ManifestNode, manifest::NodeId, path_finder::BlockValueType,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RunningScope {
    Flow {
        node_id: Option<NodeId>,
    },
    Slot {
        // slot need grandparent scope and not support injection for now.
    },
    Package {
        name: String,
        path: PathBuf,
        inject: bool,
        node_id: Option<NodeId>,
    },
}

impl RunningScope {
    pub fn workspace(&self) -> Option<PathBuf> {
        match self {
            RunningScope::Package { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    pub fn package_path(&self) -> Option<PathBuf> {
        match self {
            RunningScope::Package { path, .. } => Some(path.to_path_buf()),
            _ => None,
        }
    }

    pub fn is_inject(&self) -> bool {
        match self {
            RunningScope::Package { inject, .. } => *inject,
            _ => false,
        }
    }
}

impl Default for RunningScope {
    fn default() -> Self {
        RunningScope::Flow { node_id: None }
    }
}

pub(crate) enum RunningTarget {
    /// flow parent scope, inherit from parent flow
    Inherit,
    /// this node is spawn:true, or this node is inject to a spawn:true node
    Node(NodeId),
    /// this node is inject to a package
    InjectPackage { pkg_name: String },
    /// this node is a package block
    Package {
        pkg_name: String, // this is the package name, not the path
        // package's path, package block should always true.
        package_path: PathBuf,
        /// Some means this block is also a spawn:true node
        node_id: Option<NodeId>,
    },
}

// TODO: consider Pkg block without package path
pub(crate) fn calculate_running_target(
    node: &ManifestNode,
    injection: &Option<manifest_reader::manifest::Injection>,
    package_path: &Option<PathBuf>, // this should always be Some for package block
    block_type: BlockValueType,
) -> RunningTarget {
    if node.should_spawn() {
        match block_type {
            BlockValueType::Pkg { pkg_name, .. } => {
                if let Some(package_path) = package_path {
                    return RunningTarget::Package {
                        pkg_name,
                        package_path: package_path.clone(),
                        node_id: injection.as_ref().and_then(|inj| match &inj.target {
                            manifest_reader::manifest::InjectionTarget::Node(node_id) => {
                                Some(node_id.clone())
                            }
                            _ => None,
                        }),
                    };
                } else {
                    return RunningTarget::Node(node.node_id().clone());
                }
            }
            _ => {
                return RunningTarget::Node(node.node_id().clone());
            }
        }
    }

    match block_type {
        BlockValueType::Pkg { pkg_name, .. } => {
            if let Some(package_path) = package_path {
                return RunningTarget::Package {
                    pkg_name,
                    package_path: package_path.clone(),
                    node_id: injection.as_ref().and_then(|inj| match &inj.target {
                        manifest_reader::manifest::InjectionTarget::Node(node_id) => {
                            Some(node_id.clone())
                        }
                        _ => None,
                    }),
                };
            }
        }
        _ => {}
    }

    match injection {
        None => RunningTarget::Inherit,
        Some(injection) => match &injection.target {
            manifest_reader::manifest::InjectionTarget::Package(pkg) => {
                RunningTarget::InjectPackage {
                    pkg_name: pkg.to_owned(),
                }
            }
            manifest_reader::manifest::InjectionTarget::Node(node_id) => {
                RunningTarget::Node(node_id.to_owned())
            }
            _ => RunningTarget::Inherit,
        },
    }
}
