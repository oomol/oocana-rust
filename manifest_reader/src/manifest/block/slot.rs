use serde::Deserialize;

use crate::manifest::block::handle::{MiddleInputHandle, MiddleOutputHandle};

use super::{
    InputHandles, OutputHandles,
    handle::{convert_middle_inputs, convert_middle_outputs},
};

#[derive(Deserialize, Debug, Clone)]
struct TmpSlotBlock {
    pub inputs_def: Option<Vec<MiddleInputHandle>>,
    pub outputs_def: Option<Vec<MiddleOutputHandle>>,
}

impl From<TmpSlotBlock> for SlotBlock {
    fn from(tmp: TmpSlotBlock) -> Self {
        SlotBlock {
            inputs_def: convert_middle_inputs(tmp.inputs_def),
            outputs_def: convert_middle_outputs(tmp.outputs_def),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpSlotBlock")]
pub struct SlotBlock {
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
}
