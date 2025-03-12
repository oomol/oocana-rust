use std::{collections::HashSet, sync::Arc};

use job::{BlockInputs, BlockJobStacks, JobId};
use mainframe::reporter::ReporterMessage;
use manifest_meta::{Block, SubflowBlock, InputDefPatchMap, NodeId};

use super::{service_job, task_job};
use crate::{block_status::BlockStatusTx, flow_job, shared::Shared};

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
    pub input_values: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
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

pub fn run_block(block_args: RunBlockArgs) -> Option<BlockJobHandle> {
    let RunBlockArgs {
        block,
        shared,
        parent_flow,
        stacks,
        job_id,
        inputs,
        block_status,
        nodes,
        input_values,
        timeout_seconds,
        inputs_def_patch,
    } = block_args;

    match block {
        // block.oo.yaml type flow_block || flow.oo.yaml
        Block::Flow(flow_block) => flow_job::run_flow({
            flow_job::RunFlowArgs {
                flow_block,
                shared,
                stacks,
                flow_job_id: job_id,
                inputs,
                parent_block_status: block_status,
                nodes,
                input_values,
            }
        }),
        // block.oo.yaml type task_block
        Block::Task(task_block) => task_job::run_task_block(task_job::RunTaskBlockArgs {
            task_block,
            shared,
            parent_flow,
            stacks,
            job_id,
            inputs,
            block_status,
            timeout_seconds,
            inputs_def_patch,
        }),
        Block::Service(service_block) => {
            service_job::run_service_block(service_job::RunServiceBlockArgs {
                service_block,
                shared,
                stacks,
                job_id,
                inputs,
                block_status,
                injection_store: parent_flow.as_ref().and_then(|f| f.injection_store.clone()),
                parent_flow,
                inputs_def_patch,
            })
        }
        Block::Slot(slot_block) => {
            shared
                .reporter
                .send(mainframe::reporter::ReporterMessage::BlockFinished {
                    session_id: &shared.session_id,
                    job_id: &job_id,
                    block_path: &slot_block
                        .path
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string()),
                    stacks: stacks.vec(),
                    error: &Some("Cannot run Slot Block directly".to_string()),
                    finish_at: ReporterMessage::now(),
                });

            None
        }
    }
}
