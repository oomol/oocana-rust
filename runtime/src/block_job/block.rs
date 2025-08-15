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
