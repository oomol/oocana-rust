use serde::Deserialize;

use super::{
    handle::{to_input_handles, to_output_handles, InputHandle, OutputHandle},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Debug, Clone)]
struct TmpSlotBlock {
    pub inputs_def: Option<Vec<InputHandle>>,
    pub outputs_def: Option<Vec<OutputHandle>>,
}

impl From<TmpSlotBlock> for SlotBlock {
    fn from(tmp: TmpSlotBlock) -> Self {
        SlotBlock {
            inputs_def: to_input_handles(tmp.inputs_def),
            outputs_def: to_output_handles(tmp.outputs_def),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpSlotBlock")]
pub struct SlotBlock {
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
}
