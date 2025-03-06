use job::SessionId;
use std::{collections::HashMap, sync::Arc};

use mainframe::{reporter::ReporterTx, scheduler::SchedulerTx};
use manifest_meta::{FlowBlock, HandleName, NodesHandlesFroms, NodesHandlesTos};
use utils::output::OutputValue;

use crate::delay_abort::DelayAbortTx;

pub struct Shared {
    pub session_id: SessionId,
    pub address: String,
    pub scheduler_tx: SchedulerTx,
    pub delay_abort_tx: DelayAbortTx,
    pub reporter: ReporterTx,
    pub use_cache: bool,
}

pub struct FlowContext {
    pub flow_block: Arc<FlowBlock>,
    pub flow_inputs: Option<HashMap<HandleName, Arc<OutputValue>>>,
    pub slots_inputs_to: Option<NodesHandlesTos>,
    pub slots_outputs_from: Option<NodesHandlesFroms>,
}
