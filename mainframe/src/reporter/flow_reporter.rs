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
        Some(path) if path.ends_with("flow.oo.yaml") || path.ends_with("flow.oo.yml") => {
            FlowType::Flow
        }
        None => FlowType::Flow,
        // slotflow has some special flow paths, so we fallback all special path to slotflow
        Some(path) => {
            tracing::info!("Flow path {} with special suffix is slotFlow", path);
            FlowType::SlotFlow
        }
    }
}

#[test]
fn test_flow_type() {
    assert!(matches!(
        flow_type(&Some("test/flow.oo.yaml".to_string())),
        FlowType::Flow
    ));
    assert!(matches!(
        flow_type(&Some(
            "flow-examples/packages/array/subflows/map/subflow.oo.yaml".to_string()
        )),
        FlowType::Subflow
    ));
    assert!(matches!(
        flow_type(&Some("test/slotflow.oo.yaml".to_string())),
        FlowType::SlotFlow
    ));
    assert!(matches!(flow_type(&None), FlowType::Flow));
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
            FlowType::SlotFlow => self.tx.send(ReporterMessage::SlotflowStarted {
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

    pub fn progress(&self, progress: f32) {
        match self.flow_type {
            FlowType::Subflow => self.tx.send(ReporterMessage::SubflowBlockProgress {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                progress,
            }),
            FlowType::Flow => {}
            FlowType::SlotFlow => {}
        }
    }

    pub fn output(&self, value: Arc<OutputValue>, handle: &str) {
        match self.flow_type {
            FlowType::Subflow => self.tx.send(ReporterMessage::SubflowBlockOutput {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                output: value,
                handle,
            }),
            FlowType::Flow => {}
            FlowType::SlotFlow => self.tx.send(ReporterMessage::SlotflowOutput {
                session_id: &self.tx.session_id,
                job_id: &self.job_id,
                block_path: &self.path,
                stacks: self.stacks.vec(),
                output: value,
                handle,
            }),
        }
    }
}
