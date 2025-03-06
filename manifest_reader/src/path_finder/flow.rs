use path_clean::PathClean;
use std::path::{Path, PathBuf};
use utils::error::Result;

use super::manifest_file::find_oo_yaml;

pub fn find_flow(flow_name: &str) -> Result<PathBuf> {
    match find_oo_yaml(Path::new(flow_name), "flow") {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Flow {} not found",
            flow_name,
        ))),
    }
}
