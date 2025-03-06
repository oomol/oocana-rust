pub mod block;
pub mod node;

use crate::manifest_file_reader::read_manifest_file;

pub mod block_manifest_finder;
use self::block_manifest_finder::resolve_block_manifest_path;

use path_clean::PathClean;
use std::path::{Path, PathBuf};
use utils::error::Result;

/// Search TaskBlock `block.oo.yaml` file
pub fn resolve_task_block(
    task_name: &str, base_dir: &Path, block_search_paths: &Vec<PathBuf>,
) -> Result<PathBuf> {
    match resolve_block_manifest_path(task_name, "block", base_dir, block_search_paths) {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Task Block {} not found from {}. Search paths: {}",
            task_name,
            base_dir.to_str().unwrap_or_default(),
            block_search_paths
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ))),
    }
}

/// Read TaskBlock `block.oo.yaml` file
pub fn read_task_block(task_manifest_path: &Path) -> Result<block::TaskBlock> {
    read_manifest_file::<block::TaskBlock>(task_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Task Block manifest file {:?}",
                task_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

/// Search FlowBlock `block.oo.yaml` file
pub fn resolve_flow_block(
    flow_name: &str, base_dir: &Path, block_search_paths: &Vec<PathBuf>,
) -> Result<PathBuf> {
    match resolve_block_manifest_path(flow_name, "block", base_dir, block_search_paths) {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Flow Block {} not found from {}. Search paths: {}",
            flow_name,
            base_dir.to_str().unwrap_or_default(),
            block_search_paths
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ))),
    }
}

/// Read FlowBlock `block.oo.yaml` file
pub fn read_flow_block(flow_manifest_path: &Path) -> Result<block::FlowBlock> {
    read_manifest_file::<block::FlowBlock>(flow_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Flow Block manifest file {:?}",
                flow_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

/// Search SlotBlock `block.oo.yaml` file
pub fn resolve_slot_block(
    slot_name: &str, base_dir: &Path, block_search_paths: &Vec<PathBuf>,
) -> Result<PathBuf> {
    match resolve_block_manifest_path(slot_name, "block", base_dir, block_search_paths) {
        Some(path) => Ok(path.clean()),
        None => Err(utils::error::Error::new(&format!(
            "Slot Block {} not found from {}. Search paths: {}",
            slot_name,
            base_dir.to_str().unwrap_or_default(),
            block_search_paths
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<&str>>()
                .join(", ")
        ))),
    }
}

/// Read SlotBlock `block.oo.yaml` file
pub fn read_slot_block(slot_manifest_path: &Path) -> Result<block::SlotBlock> {
    read_manifest_file::<block::SlotBlock>(slot_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Slot Block manifest file {:?}",
                slot_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}
