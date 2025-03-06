use std::path::PathBuf;

use manifest_reader::block_manifest_reader;

use crate::{block, InputHandles, OutputHandles};

#[derive(Debug, Clone)]
pub struct SlotBlock {
    // #[serde(default)]
    // pub title: Option<String>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    // #[serde(default)]
    // pub description: Option<String>,
    // #[serde(default)]
    // pub icon: Option<String>,
    pub path: Option<PathBuf>,
    pub path_str: Option<String>,
}

impl SlotBlock {
    pub fn from_manifest(
        manifest: block_manifest_reader::block::SlotBlock, path: Option<PathBuf>,
    ) -> Self {
        let block_manifest_reader::block::SlotBlock {
            inputs_def,
            outputs_def,
        } = manifest;

        Self {
            inputs_def: block::to_input_handles(inputs_def),
            outputs_def: block::to_output_handles(outputs_def),
            path_str: path.as_ref().map(|path| path.to_string_lossy().to_string()),
            path,
        }
    }
}
