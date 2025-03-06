use std::{collections::HashSet, sync::Arc};

use job::{BlockInputs, BlockJobStacks, JobId};
use mainframe::reporter::ReporterMessage;
use manifest_meta::{Block, FlowBlock, NodeId};

use crate::{block_status::BlockStatusTx, shared::Shared};

mod applet_job;
mod flow_job;
mod task_job;

pub struct BlockJobHandle {
    pub job_id: JobId,
    _job: Box<dyn Send>,
}

impl BlockJobHandle {
    pub fn new(job_id: JobId, job: impl Send + 'static) -> Self {
        Self {
            job_id,
            _job: Box::new(job),
        }
    }
}

pub fn run_block(
    block: Block, shared: Arc<Shared>, parent_flow: Option<Arc<FlowBlock>>, stacks: BlockJobStacks,
    job_id: JobId, inputs: Option<BlockInputs>, block_status: BlockStatusTx,
    to_node: Option<NodeId>, nodes: Option<HashSet<NodeId>>, input_values: Option<String>,
) -> Option<BlockJobHandle> {
    match block {
        // block.oo.yaml type flow_block || flow.oo.yaml
        Block::Flow(flow_block) => flow_job::run_flow(
            flow_block,
            shared,
            stacks,
            job_id,
            inputs,
            block_status,
            to_node,
            nodes,
            input_values,
        ),
        // block.oo.yaml type task_block
        Block::Task(task_block) => task_job::run_task_block(
            task_block,
            shared,
            parent_flow,
            stacks,
            job_id,
            inputs,
            block_status,
        ),
        Block::Applet(applet_block) => {
            applet_job::run_applet_block(applet_block, shared, stacks, job_id, inputs, block_status)
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
