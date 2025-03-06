use std::sync::Arc;

use super::{ReporterMessage, ReporterTx};
use job::{BlockInputs, BlockJobStacks, JobId};
use utils::output::OutputValue;

pub struct FlowReporterTx {
    job_id: JobId,
    path: Option<String>,
    stacks: BlockJobStacks,
    tx: ReporterTx,
    is_block: bool,
}

fn is_flow_block(flow_path: &Option<String>) -> bool {
    if let Some(path) = flow_path {
        let path = path.as_str();
        return path.starts_with("block.");
    }
    false
}

impl FlowReporterTx {
    pub fn new(
        job_id: JobId, path: Option<String>, stacks: BlockJobStacks, tx: ReporterTx,
    ) -> Self {
        let is_block = is_flow_block(&path);
        Self {
            job_id,
            path,
            stacks,
            tx,
            is_block: is_block,
        }
    }

    pub fn started(&self, inputs: &Option<BlockInputs>) {
        match self.is_block {
            true => self.tx.send(ReporterMessage::FlowBlockStarted {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                inputs: &inputs,
                stacks: &self.stacks.vec(),
                create_at: ReporterMessage::now(),
            }),
            false => self.tx.send(ReporterMessage::FlowStarted {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                flow_path: &self.path,
                stacks: &self.stacks.vec(),
                create_at: ReporterMessage::now(),
            }),
        }
    }

    pub fn done(&self, error: &Option<String>) {
        match self.is_block {
            true => self.tx.send(ReporterMessage::FlowBlockFinished {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: &self.stacks.vec(),
                error: error,
                finish_at: ReporterMessage::now(),
            }),
            false => self.tx.send(ReporterMessage::FlowFinished {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                flow_path: &self.path,
                stacks: &self.stacks.vec(),
                error: error,
                finish_at: ReporterMessage::now(),
            }),
        }
    }

    pub fn result(&self, result: Arc<OutputValue>, handle: &str) {
        match self.is_block {
            true => self.tx.send(ReporterMessage::FlowBlockOutput {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: &self.stacks.vec(),
                output: result,
                handle,
            }),
            false => self.tx.send(ReporterMessage::FlowOutput {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                flow_path: &self.path,
                stacks: &self.stacks.vec(),
                output: result,
                handle,
            }),
        }
    }
}
