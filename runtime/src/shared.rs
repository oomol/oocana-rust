use job::SessionId;

use mainframe::{reporter::ReporterTx, scheduler::SchedulerTx};

use crate::delay_abort::DelayAbortTx;

pub struct Shared {
    pub session_id: SessionId,
    pub address: String,
    pub scheduler_tx: SchedulerTx,
    pub delay_abort_tx: DelayAbortTx,
    pub reporter: ReporterTx,
    pub use_cache: bool,
}
