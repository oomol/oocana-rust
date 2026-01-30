use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use mainframe::reporter::ReporterMessage;
use manifest_meta::{
    InputDefPatchMap, InputHandles, NodeId, OutputHandles, ServiceBlock, Slot, SlotBlock,
    SubflowBlock, TaskBlock,
};

use crate::{
    block_job::{self, BlockJobHandle},
    block_status::BlockStatusTx,
    flow_job::{self, NodeInputValues},
    shared::Shared,
};

pub struct CommonJobParameters {
    pub job_id: JobId,
    pub shared: Arc<Shared>,
    pub inputs: Option<BlockInputs>,
    pub stacks: BlockJobStacks,
    pub block_status: BlockStatusTx,
    pub scope: RuntimeScope,
}

/// Maximum allowed recursion depth for job execution.
/// Exceeding this limit will cause the job to fail with an error.
pub const MAX_RECURSION_DEPTH: usize = 50;

#[allow(clippy::large_enum_variant)]
pub enum JobParams {
    Flow {
        flow_block: Arc<RwLock<SubflowBlock>>,
        nodes: Option<HashSet<NodeId>>,
        parent_scope: RuntimeScope,
        node_value_store: NodeInputValues,
        slot_blocks: Option<HashMap<NodeId, Slot>>,
        path_finder: manifest_reader::path_finder::BlockPathFinder,
        common: CommonJobParameters,
        vault_client: Arc<Option<vault::VaultClient>>,
    },
    Task {
        task_block: Arc<TaskBlock>,
        inputs_def: Option<InputHandles>, // block's inputs def will miss additional inputs added on node
        outputs_def: Option<OutputHandles>, // block's outputs def will miss additional outputs added on node
        parent_flow: Option<Arc<RwLock<SubflowBlock>>>,
        timeout: Option<u64>,
        inputs_def_patch: Option<InputDefPatchMap>,
        common: CommonJobParameters,
    },
    Service {
        service_block: Arc<ServiceBlock>,
        parent_flow: Option<Arc<RwLock<SubflowBlock>>>,
        inputs_def_patch: Option<InputDefPatchMap>,
        common: CommonJobParameters,
    },
    Slot {
        #[allow(dead_code)]
        slot_block: Arc<SlotBlock>,
        common: CommonJobParameters,
    },
    Condition {
        condition_block: Arc<manifest_meta::ConditionBlock>,
        output_def: Option<manifest_meta::OutputHandle>,
        common: CommonJobParameters,
    },
}

impl JobParams {
    fn common(&self) -> &CommonJobParameters {
        match self {
            JobParams::Flow { common, .. } => common,
            JobParams::Task { common, .. } => common,
            JobParams::Service { common, .. } => common,
            JobParams::Slot { common, .. } => common,
            JobParams::Condition { common, .. } => common,
        }
    }
}

pub fn run_job(params: JobParams) -> Option<BlockJobHandle> {
    let depth = params.common().stacks.depth();
    if depth >= MAX_RECURSION_DEPTH {
        let common = params.common();
        common.shared.reporter.send(ReporterMessage::BlockFinished {
            session_id: &common.shared.session_id,
            job_id: &common.job_id,
            block_path: &None,
            stacks: common.stacks.vec(),
            error: Some(format!(
                "Maximum recursion depth exceeded: {} (limit: {})",
                depth, MAX_RECURSION_DEPTH
            )),
            result: None,
            finish_at: ReporterMessage::now(),
        });
        return None;
    }

    match params {
        JobParams::Flow {
            flow_block,
            nodes,
            parent_scope,
            node_value_store,
            slot_blocks,
            path_finder,
            common,
            vault_client,
        } => flow_job::execute_flow_job(flow_job::FlowJobParameters {
            flow_block,
            shared: common.shared,
            stacks: common.stacks,
            flow_job_id: common.job_id,
            inputs: common.inputs,
            parent_block_status: common.block_status,
            nodes,
            parent_scope,
            node_value_store,
            scope: common.scope,
            slot_blocks: slot_blocks.unwrap_or_default(),
            path_finder,
            vault_client,
        }),
        JobParams::Task {
            task_block,
            parent_flow,
            timeout,
            inputs_def_patch,
            inputs_def,
            outputs_def,
            common,
        } => crate::block_job::execute_task_job(crate::block_job::TaskJobParameters {
            executor: task_block.executor.clone(),
            block_path: task_block.path_str(),
            inputs_def,
            outputs_def,
            shared: common.shared,
            flow_path: parent_flow.as_ref().map(|f| f.read().unwrap().path_str.clone()),
            injection_store: parent_flow.as_ref().and_then(|f| f.read().unwrap().injection_store.clone()),
            dir: block_job::block_dir(&task_block, parent_flow.as_ref(), Some(&common.scope)),
            stacks: common.stacks,
            job_id: common.job_id,
            inputs: common.inputs,
            block_status: common.block_status,
            scope: common.scope,
            timeout,
            inputs_def_patch,
        }),
        JobParams::Service {
            service_block,
            parent_flow,
            inputs_def_patch,
            common,
        } => crate::block_job::execute_service_job(crate::block_job::ServiceJobParameters {
            service_block,
            shared: common.shared,
            stacks: common.stacks,
            job_id: common.job_id,
            inputs: common.inputs,
            block_status: common.block_status,
            injection_store: parent_flow.as_ref().and_then(|f| f.read().unwrap().injection_store.clone()),
            parent_flow,
            scope: common.scope,
            inputs_def_patch,
        }),
        JobParams::Slot { common: shared, .. } => {
            shared
                .shared
                .reporter
                .send(mainframe::reporter::ReporterMessage::BlockFinished {
                    session_id: &shared.shared.session_id,
                    job_id: &shared.job_id,
                    block_path: &None,
                    stacks: shared.stacks.vec(),
                    error: Some("Cannot run Slot Block directly".to_string()),
                    result: None,
                    finish_at: ReporterMessage::now(),
                });
            None
        }
        JobParams::Condition {
            condition_block,
            common,
            output_def,
        } => crate::block_job::execute_condition_job(crate::block_job::ConditionJobParameters {
            condition_block,
            shared: common.shared,
            stacks: common.stacks,
            job_id: common.job_id,
            inputs: common.inputs,
            block_status: common.block_status,
            scope: common.scope,
            output_def,
        }),
    }
}
