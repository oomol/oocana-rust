use std::collections::HashMap;

use serde::Deserialize;

use crate::manifest::{
    block::handle::{MiddleInputHandle, MiddleOutputHandle},
    Node, NodeId, NodeInputFrom,
};

use super::{
    handle::{convert_middle_inputs, convert_middle_outputs},
    InputHandles, OutputHandles,
};

#[derive(Deserialize, Debug, Clone)]
pub struct TmpSubflowBlock {
    pub description: Option<String>,
    #[serde(default)]
    pub nodes: Vec<Node>,
    pub outputs_from: Option<Vec<NodeInputFrom>>,
    pub inputs_def: Option<Vec<MiddleInputHandle>>,
    pub outputs_def: Option<Vec<MiddleOutputHandle>>,
    pub injection: Option<HashMap<String, String>>,
    pub forward_previews: Option<Vec<NodeId>>,
}

impl From<TmpSubflowBlock> for SubflowBlock {
    fn from(tmp: TmpSubflowBlock) -> Self {
        SubflowBlock {
            description: tmp.description,
            nodes: tmp.nodes,
            outputs_from: tmp.outputs_from,
            inputs_def: convert_middle_inputs(tmp.inputs_def),
            outputs_def: convert_middle_outputs(tmp.outputs_def),
            injection: tmp.injection,
            forward_previews: tmp.forward_previews,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(from = "TmpSubflowBlock")]
pub struct SubflowBlock {
    pub description: Option<String>,
    pub nodes: Vec<Node>,
    pub outputs_from: Option<Vec<NodeInputFrom>>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub injection: Option<HashMap<String, String>>,
    pub forward_previews: Option<Vec<NodeId>>,
}
