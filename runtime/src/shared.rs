use job::SessionId;
use std::{collections::HashMap, sync::Arc};

use mainframe::{reporter::ReporterTx, scheduler::SchedulerTx};
use manifest_meta::{HandleName, SubflowBlock};
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
    pub flow_block: Arc<SubflowBlock>,
    pub flow_inputs: Option<HashMap<HandleName, Arc<OutputValue>>>,
}
