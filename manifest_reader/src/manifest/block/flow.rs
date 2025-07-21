use std::collections::HashMap;

use serde::Deserialize;

use crate::manifest::{
    block::handle::{MiddleInputHandle, MiddleOutputHandle},
    Node, NodeInputFrom,
};

use super::{
    handle::{to_input_handles, to_output_handles},
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
}

impl From<TmpSubflowBlock> for SubflowBlock {
    fn from(tmp: TmpSubflowBlock) -> Self {
        SubflowBlock {
            description: tmp.description,
            nodes: tmp.nodes,
            outputs_from: tmp.outputs_from,
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
            injection: tmp.injection,
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
}
