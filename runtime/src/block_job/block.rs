use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use mainframe::reporter::ReporterMessage;
use manifest_meta::{
    Block, InputDefPatchMap, NodeId, ServiceBlock, Slot, SlotBlock, SubflowBlock, TaskBlock,
};

use super::{service_job, task_job};
use crate::{
    block_status::BlockStatusTx,
    flow_job::{self, NodeInputValues},
    shared::Shared,
};

pub struct BlockJobHandle {
    // TODO: Remove this field
    #[allow(dead_code)]
    pub job_id: JobId,
    _job: Box<dyn Send>,
}

impl BlockJobHandle {
    // TODO: Remove this field
    #[allow(dead_code)]
    pub fn get_job_id(&self) -> &JobId {
        &self.job_id
    }
}

impl BlockJobHandle {
    pub fn new(job_id: JobId, job: impl Send + 'static) -> Self {
        Self {
            job_id,
            _job: Box::new(job),
        }
    }
}

pub struct RunBlockArgs {
    pub block: Block,
    pub shared: Arc<Shared>,
    pub parent_flow: Option<Arc<SubflowBlock>>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    pub nodes: Option<HashSet<NodeId>>,
    pub node_value_store: Option<NodeInputValues>,
    pub parent_scope: RuntimeScope,
    pub scope: RuntimeScope,
    pub timeout: Option<u64>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
    pub slot_blocks: Option<HashMap<NodeId, Slot>>,
    pub path_finder: manifest_reader::path_finder::BlockPathFinder,
}

pub struct FindUpstreamArgs {
    pub flow_block: Arc<SubflowBlock>,
    pub use_cache: bool,
    pub nodes: Option<HashSet<NodeId>>,
}

pub fn find_upstream(args: FindUpstreamArgs) -> (Vec<String>, Vec<String>, Vec<String>) {
    let FindUpstreamArgs {
        flow_block,
        nodes,
        use_cache,
    } = args;
    let upstream_args = flow_job::UpstreamArgs {
        flow_block,
        use_cache,
        nodes,
    };
    flow_job::find_upstream(upstream_args)
}

pub struct CommonJobParameters {
    pub job_id: JobId,
    pub shared: Arc<Shared>,
    pub inputs: Option<BlockInputs>,
    pub stacks: BlockJobStacks,
    pub block_status: BlockStatusTx,
    pub scope: RuntimeScope,
}

pub enum JobParams {
    Flow {
        flow_block: Arc<SubflowBlock>,
        nodes: Option<HashSet<NodeId>>,
        parent_scope: RuntimeScope,
        node_value_store: NodeInputValues,
        slot_blocks: Option<HashMap<NodeId, Slot>>,
        path_finder: manifest_reader::path_finder::BlockPathFinder,
        common: CommonJobParameters,
    },
    Task {
        task_block: Arc<TaskBlock>,
        parent_flow: Option<Arc<SubflowBlock>>,
        scope: RuntimeScope,
        timeout: Option<u64>,
        inputs_def_patch: Option<InputDefPatchMap>,
        common: CommonJobParameters,
    },
    Service {
        service_block: Arc<ServiceBlock>,
        parent_flow: Option<Arc<SubflowBlock>>,
        inputs_def_patch: Option<InputDefPatchMap>,
        shared: CommonJobParameters,
    },
    Slot {
        slot_block: Arc<SlotBlock>,
        scope: RuntimeScope,
        inputs_def_patch: Option<InputDefPatchMap>,
        common: CommonJobParameters,
    },
}

pub fn run_job(args: JobParams) -> Option<BlockJobHandle> {
    match args {
        JobParams::Flow {
            flow_block,
            nodes,
            parent_scope,
            node_value_store,
            slot_blocks,
            path_finder,
            common,
        } => flow_job::run_flow(flow_job::RunFlowArgs {
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
        }),
        JobParams::Task {
            task_block,
            parent_flow,
            timeout,
            scope,
            inputs_def_patch,
            common,
        } => task_job::run_task_block(task_job::RunTaskBlockArgs {
            task_block,
            shared: common.shared,
            parent_flow,
            stacks: common.stacks,
            job_id: common.job_id,
            inputs: common.inputs,
            block_status: common.block_status,
            scope,
            timeout,
            inputs_def_patch,
        }),
        JobParams::Service {
            service_block,
            parent_flow,
            inputs_def_patch,
            shared: common,
        } => service_job::run_service_block(service_job::RunServiceBlockArgs {
            service_block,
            shared: common.shared,
            stacks: common.stacks,
            job_id: common.job_id,
            inputs: common.inputs,
            block_status: common.block_status,
            injection_store: parent_flow.as_ref().and_then(|f| f.injection_store.clone()),
            parent_flow: parent_flow,
            scope: common.scope,
            inputs_def_patch,
        }),
        JobParams::Slot {
            slot_block,
            common: shared,
            ..
        } => {
            shared
                .shared
                .reporter
                .send(mainframe::reporter::ReporterMessage::BlockFinished {
                    session_id: &shared.shared.session_id,
                    job_id: &shared.job_id,
                    block_path: &slot_block
                        .path
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string()),
                    stacks: shared.stacks.vec(),
                    error: Some("Cannot run Slot Block directly".to_string()),
                    result: None,
                    finish_at: ReporterMessage::now(),
                });
            None
        }
    }
}
