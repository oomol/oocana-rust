use std::collections::HashMap;

use job::{BlockInputs, BlockJobStacks, JobId};
use manifest_meta::JsonValue;

use super::{ReporterMessage, ReporterTx};

pub struct BlockReporterTx {
    job_id: JobId,
    block_path: Option<String>,
    stacks: BlockJobStacks,
    tx: ReporterTx,
}

impl BlockReporterTx {
    pub fn new(
        job_id: JobId,
        block_path: Option<String>,
        stacks: BlockJobStacks,
        tx: ReporterTx,
    ) -> Self {
        Self {
            job_id,
            block_path,
            stacks,
            tx,
        }
    }

    pub fn started(&self, inputs: &Option<BlockInputs>) {
        self.tx.send(ReporterMessage::BlockStarted {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            inputs,
            create_at: ReporterMessage::now(),
        });
    }

    pub fn finished(&self, result: Option<HashMap<String, JsonValue>>, error: Option<String>) {
        self.tx.send(ReporterMessage::BlockFinished {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            error,
            result,
            finish_at: ReporterMessage::now(),
        });
    }

    pub fn output(&self, result: &JsonValue, handle: &str) {
        self.tx.send(ReporterMessage::BlockOutput {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            output: result,
            handle,
        });
    }

    pub fn outputs(&self, outputs: &HashMap<String, JsonValue>) {
        self.tx.send(ReporterMessage::BlockOutputs {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            outputs,
        });
    }

    pub fn log(&self, log: &str, stdio: &str) {
        self.tx.send(ReporterMessage::BlockLog {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            log,
            stdio,
        });
    }

    pub fn error(&self, error: &str) {
        self.tx.send(ReporterMessage::BlockError {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            error,
        });
    }
}
