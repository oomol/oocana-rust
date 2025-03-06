use crate::{
    block_manifest_reader::{applet, block_manifest_finder::resolve_block_manifest_path},
    manifest_file_reader::read_manifest_file,
};

use path_clean::PathClean;
use std::path::{Path, PathBuf};
use utils::error::Result;

pub fn resolve_applet(
    applet_name: &str, base_dir: &Path, block_search_paths: &Vec<PathBuf>,
) -> Result<PathBuf> {
    match resolve_block_manifest_path(applet_name, "applet", base_dir, block_search_paths) {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Applet {} not found",
            applet_name,
        ))),
    }
}

pub fn read_applet(applet_manifest_path: &Path) -> Result<applet::Applet> {
    read_manifest_file::<applet::Applet>(applet_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read applet manifest file {:?}",
                applet_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}
