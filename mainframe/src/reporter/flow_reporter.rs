use std::sync::Arc;

use super::{ReporterMessage, ReporterTx};
use job::{BlockInputs, BlockJobStacks, JobId};
use utils::output::OutputValue;

pub struct FlowReporterTx {
    job_id: JobId,
    path: Option<String>,
    stacks: BlockJobStacks,
    tx: ReporterTx,
    flow_type: FlowType,
}

enum FlowType {
    Flow,
    Subflow,
    SlotFlow,
}

fn flow_type(flow_path: &Option<String>) -> FlowType {
    match flow_path {
        Some(path) if path.ends_with("subflow.oo.yaml") || path.ends_with("subflow.oo.yml") => {
            FlowType::Subflow
        }
        Some(path) if path.ends_with("slotflow.oo.yaml") || path.ends_with("slotflow.oo.yml") => {
            FlowType::SlotFlow
        }
        _ => FlowType::Flow,
    }
}

impl FlowReporterTx {
    pub fn new(
        job_id: JobId,
        path: Option<String>,
        stacks: BlockJobStacks,
        tx: ReporterTx,
    ) -> Self {
        let flow_type = flow_type(&path);
        Self {
            job_id,
            path,
            stacks,
            tx,
            flow_type,
        }
    }

    pub fn started(&self, inputs: &Option<BlockInputs>) {
        match self.flow_type {
            FlowType::Subflow => self.tx.send(ReporterMessage::SubflowBlockStarted {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                inputs,
                stacks: self.stacks.vec(),
                create_at: ReporterMessage::now(),
            }),
            FlowType::SlotFlow => self.tx.send(ReporterMessage::SlotflowBlockStarted {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                inputs,
                stacks: self.stacks.vec(),
                create_at: ReporterMessage::now(),
            }),
            FlowType::Flow => self.tx.send(ReporterMessage::FlowStarted {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                flow_path: &self.path,
                stacks: self.stacks.vec(),
                create_at: ReporterMessage::now(),
            }),
        }
    }

    pub fn will_run_nodes(&self, start: &Vec<String>, mid: &Vec<String>, end: &Vec<String>) {
        if matches!(self.flow_type, FlowType::Flow) {
            self.tx.send(ReporterMessage::FlowNodesWillRun {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                flow_path: &self.path,
                stacks: self.stacks.vec(),
                mid_nodes: mid,
                start_nodes: start,
                end_nodes: end,
            });
        }
    }

    pub fn done(&self, error: &Option<String>) {
        match self.flow_type {
            FlowType::Subflow => self.tx.send(ReporterMessage::SubflowBlockFinished {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                error,
                finish_at: ReporterMessage::now(),
            }),
            FlowType::Flow => self.tx.send(ReporterMessage::FlowFinished {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                flow_path: &self.path,
                stacks: self.stacks.vec(),
                error,
                finish_at: ReporterMessage::now(),
            }),
            FlowType::SlotFlow => self.tx.send(ReporterMessage::SlotflowFinished {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                error,
                finish_at: ReporterMessage::now(),
            }),
        }
    }

    pub fn result(&self, result: Arc<OutputValue>, handle: &str) {
        match self.flow_type {
            FlowType::Subflow => self.tx.send(ReporterMessage::SubflowBlockOutput {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                output: result,
                handle,
            }),
            FlowType::Flow => {}
            FlowType::SlotFlow => self.tx.send(ReporterMessage::SlotflowBlockOutput {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                output: result,
                handle,
            }),
        }
    }
}
