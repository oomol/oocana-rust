use serde::{Deserialize, Serialize};

use super::{
    InputHandles, OutputHandles,
    handle::{InputHandle, OutputHandle, to_input_handles, to_output_handles},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TmpServiceBlock {
    pub name: String,
    pub inputs_def: Option<Vec<InputHandle>>,
    pub outputs_def: Option<Vec<OutputHandle>>,
}

impl From<TmpServiceBlock> for ServiceBlock {
    fn from(tmp: TmpServiceBlock) -> Self {
        ServiceBlock {
            name: tmp.name,
            inputs_def: to_input_handles(tmp.inputs_def),
            outputs_def: to_output_handles(tmp.outputs_def),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(from = "TmpServiceBlock")]
pub struct ServiceBlock {
    pub name: String,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
}
