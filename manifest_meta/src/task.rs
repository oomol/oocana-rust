use std::path::PathBuf;

use manifest_reader::block_manifest_reader;

use crate::{block, InputHandles, OutputHandles, TaskBlockEntry};

#[derive(Debug, Clone)]
pub struct TaskBlock {
    pub entry: TaskBlockEntry,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub path: Option<PathBuf>,
    pub path_str: Option<String>,
}

impl TaskBlock {
    pub fn from_manifest(
        manifest: block_manifest_reader::block::TaskBlock, path: Option<PathBuf>,
    ) -> Self {
        let block_manifest_reader::block::TaskBlock {
            entry,
            inputs_def,
            outputs_def,
        } = manifest;

        Self {
            entry,
            inputs_def: block::to_input_handles(inputs_def),
            outputs_def: block::to_output_handles(outputs_def),
            path_str: path.as_ref().map(|path| path.to_string_lossy().to_string()),
            path,
        }
    }
}
