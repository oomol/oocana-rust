use manifest_meta::NodeId;
use std::path::PathBuf;
use utils::calculate_short_hash;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunningPackageScope {
    pub package_path: PathBuf,
    pub node_id: Option<NodeId>,
    pub enable_layer: bool,
}

impl RunningPackageScope {
    pub fn identifier(&self) -> String {
        let str = match &self.node_id {
            Some(node_id) => format!("{}-{}", self.package_path.display(), node_id),
            None => self.package_path.display().to_string(),
        };
        calculate_short_hash(&str, 16)
    }

    pub fn package_path(&self) -> &PathBuf {
        &self.package_path
    }

    pub fn node_id(&self) -> &Option<NodeId> {
        &self.node_id
    }

    pub fn workspace(&self) -> &PathBuf {
        self.package_path()
    }

    pub fn need_layer(&self) -> bool {
        self.enable_layer
    }
}
