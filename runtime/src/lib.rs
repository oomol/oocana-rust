mod block_job;

pub mod block_status;
pub mod delay_abort;
pub mod shared;

use std::sync::Arc;

use job::{BlockJobStacks, JobId};
use manifest_meta::{flow_reader, Block, BlockPathResolver, BlockReader, NodeId};
use utils::error::Result;

pub async fn run(
    shared: Arc<shared::Shared>, block_name: &str, block_reader: BlockReader,
    resolver: BlockPathResolver, job_id: Option<JobId>, to_node: Option<NodeId>,
) -> Result<()> {
    let (block_status_tx, block_status_rx) = block_status::create();
    let job_id = job_id.unwrap_or_else(JobId::random);
    let stacks = BlockJobStacks::new();

    let block = read_flow_or_block(block_name, block_reader, resolver)?;

    let block_path = block
        .path_str()
        .map(|p| p.to_owned())
        .unwrap_or_else(|| block_name.to_string());

    shared.reporter.session_started(&block_path);

    let handle = block_job::run_block(
        block,
        Arc::clone(&shared),
        None,
        stacks,
        job_id,
        None,
        block_status_tx,
        to_node,
    );

    while let Some(status) = block_status_rx.recv().await {
        match status {
            block_status::Status::Result { .. } => break,
            block_status::Status::Done { .. } => break,
        };
    }

    shared.reporter.session_finished(&block_path);

    drop(handle);

    Ok(())
}

fn read_flow_or_block(
    block_name: &str, mut block_reader: BlockReader, mut resolver: BlockPathResolver,
) -> Result<Block> {
    if let Ok(flow_path) = flow_reader::resolve_flow(block_name) {
        return flow_reader::read_flow(&flow_path, &mut block_reader, &mut resolver)
            .map(|flow| Block::Flow(Arc::new(flow)));
    }

    block_reader.resolve_block(block_name, &mut resolver)
}
