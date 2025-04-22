use super::search_paths::{search_block_manifest, BlockManifestParams};
use path_clean::PathClean;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use utils::error::Result;

pub struct TaskBlockManifestParams<'a> {
    pub value: &'a str,
    pub base_dir: &'a Path,
    pub search_paths: &'a Vec<PathBuf>,
    pub pkg_version: &'a HashMap<String, String>,
}

pub fn find_task_block(params: TaskBlockManifestParams) -> Result<PathBuf> {
    let TaskBlockManifestParams {
        value,
        base_dir,
        search_paths,
        pkg_version,
    } = params;
    if let Some(path) = search_block_manifest(BlockManifestParams {
        value,
        base_name: "block",
        block_dir: "blocks",
        working_dir: base_dir,
        search_paths,
        pkg_version,
    }) {
        return Ok(path.clean());
    }

    if let Some(path) = search_block_manifest(BlockManifestParams {
        value,
        base_name: "task",
        block_dir: "tasks",
        working_dir: base_dir,
        search_paths,
        pkg_version,
    }) {
        return Ok(path.clean());
    }

    Err(utils::error::Error::new(&format!(
        "Task block {} could not be found in either {} or in the search paths: {}",
        value,
        base_dir.to_str().unwrap_or_default(),
        search_paths
            .iter()
            .filter_map(|p| p.to_str())
            .collect::<Vec<&str>>()
            .join(", ")
    )))
}

pub struct SubflowBlockManifestParams<'a> {
    pub value: &'a str,
    pub base_dir: &'a Path,
    pub search_paths: &'a Vec<PathBuf>,
    pub pkg_version: &'a HashMap<String, String>,
}

pub fn find_flow_block(params: SubflowBlockManifestParams) -> Result<PathBuf> {
    let SubflowBlockManifestParams {
        value,
        base_dir,
        search_paths,
        pkg_version,
    } = params;
    match search_block_manifest(BlockManifestParams {
        value,
        base_name: "subflow",
        block_dir: "subflows",
        working_dir: base_dir,
        search_paths,
        pkg_version,
    }) {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Flow block {} could not be found in either {} or in the search paths: {}",
            value,
            base_dir.to_str().unwrap_or_default(),
            search_paths
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ))),
    }
}

pub struct SlotBlockManifestParams<'a> {
    pub value: &'a str,
    pub base_dir: &'a Path,
    pub search_paths: &'a Vec<PathBuf>,
    pub pkg_version: &'a HashMap<String, String>,
}

pub fn find_slot_block(params: SlotBlockManifestParams) -> Result<PathBuf> {
    let SlotBlockManifestParams {
        value,
        base_dir,
        search_paths,
        pkg_version,
    } = params;
    match search_block_manifest(BlockManifestParams {
        value,
        base_name: "slot",
        block_dir: "slots",
        working_dir: base_dir,
        search_paths,
        pkg_version,
    }) {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Slot block {} could not be found in either {} or in the search paths: {}",
            value,
            base_dir.to_str().unwrap_or_default(),
            search_paths
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ))),
    }
}
