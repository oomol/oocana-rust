use job::SessionId;

use mainframe::{reporter::ReporterTx, scheduler::SchedulerTx};

use crate::delay_abort::DelayAbortTx;
use crate::remote_task_config::RemoteTaskConfig;

pub struct Shared {
    pub session_id: SessionId,
    pub address: String,
    pub scheduler_tx: SchedulerTx,
    pub delay_abort_tx: DelayAbortTx,
    pub reporter: ReporterTx,
    pub use_cache: bool,
    pub remote_task_config: Option<RemoteTaskConfig>,
}
