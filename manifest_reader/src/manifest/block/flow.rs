use std::collections::HashMap;

use serde::Deserialize;

use crate::manifest::{Node, NodeInputFrom};

use super::{
    handle::{to_input_handles, to_output_handles, InputHandle, OutputHandle},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Debug, Clone)]
pub struct TmpSubflowBlock {
    pub nodes: Vec<Node>,
    pub outputs_from: Option<Vec<NodeInputFrom>>,
    pub inputs_def: Option<Vec<InputHandle>>,
    pub outputs_def: Option<Vec<OutputHandle>>,
    pub injection: Option<HashMap<String, String>>,
}

impl From<TmpSubflowBlock> for SubflowBlock {
    fn from(tmp: TmpSubflowBlock) -> Self {
        SubflowBlock {
            nodes: tmp.nodes,
            outputs_from: tmp.outputs_from,
            inputs_def: to_input_handles(tmp.inputs_def),
            outputs_def: to_output_handles(tmp.outputs_def),
            injection: tmp.injection,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpSubflowBlock")]
pub struct SubflowBlock {
    pub nodes: Vec<Node>,
    pub outputs_from: Option<Vec<NodeInputFrom>>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub injection: Option<HashMap<String, String>>,
}
