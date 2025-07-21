use serde::Deserialize;

use crate::manifest::block::handle::{MiddleInputHandle, MiddleOutputHandle};

use super::{
    handle::{to_input_handles, to_output_handles},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Debug, Clone)]
struct TmpSlotBlock {
    pub inputs_def: Option<Vec<MiddleInputHandle>>,
    pub outputs_def: Option<Vec<MiddleOutputHandle>>,
}

impl From<TmpSlotBlock> for SlotBlock {
    fn from(tmp: TmpSlotBlock) -> Self {
        SlotBlock {
            inputs_def: to_input_handles(tmp.inputs_def.map(|v| {
                v.into_iter()
                    .filter_map(|h| match h {
                        MiddleInputHandle::Input(handle) => Some(handle),
                        MiddleInputHandle::Group { .. } => None,
                    })
                    .collect()
            })),
            outputs_def: to_output_handles(tmp.outputs_def.map(|v| {
                v.into_iter()
                    .filter_map(|h| match h {
                        MiddleOutputHandle::Output(handle) => Some(handle),
                        MiddleOutputHandle::Group { .. } => None,
                    })
                    .collect()
            })),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpSlotBlock")]
pub struct SlotBlock {
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
}
