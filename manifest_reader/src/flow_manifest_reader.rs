use crate::manifest_file_reader::{read_manifest_file, resolve_path};

// FIXME 先复用 FlowBlock 格式
use crate::block_manifest_reader::block;

use path_clean::PathClean;
use std::path::{Path, PathBuf};
use utils::error::Result;

/// Search Flow `flow.oo.yaml` file
pub fn resolve_flow(flow_name: &str) -> Result<PathBuf> {
    match resolve_path(Path::new(flow_name), "flow") {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Flow {} not found",
            flow_name,
        ))),
    }
}

// FIXME 先复用 FlowBlock 格式
/// Read Flow `flow.oo.yaml` file
pub fn read_flow(flow_manifest_path: &Path) -> Result<block::FlowBlock> {
    read_manifest_file::<block::FlowBlock>(flow_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read flow manifest file {:?}",
                flow_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}
