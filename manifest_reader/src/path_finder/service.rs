use path_clean::PathClean;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use utils::error::Result;

use super::{
    calculate_block_value_type,
    search_paths::{search_block_manifest, BlockManifestParams},
};

pub struct ServiceManifestParams<'a> {
    pub value: &'a str,
    pub base_dir: &'a Path,
    pub block_search_paths: &'a Vec<PathBuf>,
    pub pkg_version: &'a HashMap<String, String>,
}

pub fn find_service(params: ServiceManifestParams) -> Result<PathBuf> {
    let ServiceManifestParams {
        value,
        base_dir,
        block_search_paths,
        pkg_version,
    } = params;
    if let Some(path) = search_block_manifest(BlockManifestParams {
        block_value: calculate_block_value_type(value),
        file_prefix: "service",
        block_dir: "services",
        working_dir: base_dir,
        search_paths: block_search_paths,
        pkg_version,
    }) {
        return Ok(path.clean());
    }

    if let Some(path) = search_block_manifest(BlockManifestParams {
        block_value: calculate_block_value_type(value),
        file_prefix: "service",
        block_dir: "blocks",
        working_dir: base_dir,
        search_paths: block_search_paths,
        pkg_version,
    }) {
        return Ok(path.clean());
    }

    Err(utils::error::Error::new(&format!(
        "Service {} could not be found in either {} or in the search paths: {}",
        value,
        base_dir.to_str().unwrap_or_default(),
        block_search_paths
            .iter()
            .filter_map(|p| p.to_str())
            .collect::<Vec<&str>>()
            .join(", ")
    )))
}
