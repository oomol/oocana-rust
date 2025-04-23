use std::path::PathBuf;

use manifest_reader::manifest::{self, InputHandles, OutputHandles};

#[derive(Debug, Clone)]
pub struct SlotBlock {
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub path: Option<PathBuf>,
    pub path_str: Option<String>,
    pub package_path: Option<PathBuf>,
}

impl SlotBlock {
    pub fn from_manifest(
        manifest: manifest::SlotBlock,
        path: Option<PathBuf>,
        package_path: Option<PathBuf>,
    ) -> Self {
        let manifest::SlotBlock {
            inputs_def,
            outputs_def,
        } = manifest;

        Self {
            inputs_def,
            outputs_def,
            path_str: path.as_ref().map(|path| path.to_string_lossy().to_string()),
            path,
            package_path,
        }
    }
}
