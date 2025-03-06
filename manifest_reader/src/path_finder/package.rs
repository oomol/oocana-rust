use std::path::{Path, PathBuf};

use super::manifest_file::find_oo_yaml_in_dir;

/// return package.oo.<yaml|yml> file's path
pub fn find_package_file<P: AsRef<Path>>(dir_path: P) -> Option<PathBuf> {
    find_oo_yaml_in_dir(dir_path.as_ref(), "package")
}
