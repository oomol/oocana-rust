use crate::manifest_file_reader;
use path_clean::PathClean;
use serde::Deserialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use utils::error::Result;

#[derive(Deserialize, Debug, Clone)]
pub struct PackageMeta {
    #[serde(default)]
    pub exports: Option<HashMap<String, String>>,
}

/// Resolve `package.yaml` file from dir.
pub fn resolve_path_from_dir(dir_path: &Path) -> Option<PathBuf> {
    manifest_file_reader::resolve_path_from_dir(dir_path, "package")
}

/// Read `package.yaml` file.
pub fn read_manifest_file(file_path: &Path) -> Result<PackageMeta> {
    manifest_file_reader::read_manifest_file::<PackageMeta>(file_path).map_err(|err| {
        utils::error::Error::with_source(
            &format!("Unable to read package {:?}", file_path.clean()),
            Box::new(err),
        )
    })
}
