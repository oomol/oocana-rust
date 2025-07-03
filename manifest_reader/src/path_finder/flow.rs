use path_clean::PathClean;
use std::path::{Path, PathBuf};
use utils::error::Result;

use super::manifest_file::find_oo_yaml;

pub fn find_flow<P: AsRef<Path>>(flow_path: P) -> Result<PathBuf> {
    match find_oo_yaml(flow_path.as_ref(), "flow") {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Flow {:?} not found",
            flow_path.as_ref(),
        ))),
    }
}
