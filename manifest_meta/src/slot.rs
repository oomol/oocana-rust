use manifest_reader::manifest::{self, InputHandles, OutputHandles};

#[derive(Debug, Clone)]
pub struct SlotBlock {
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
}

impl SlotBlock {
    pub fn from_manifest(manifest: manifest::SlotBlock) -> Self {
        let manifest::SlotBlock {
            inputs_def,
            outputs_def,
        } = manifest;

        Self {
            inputs_def,
            outputs_def,
        }
    }
}
