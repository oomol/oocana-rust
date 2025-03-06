use std::path::Path;
use utils::error::Result;

use crate::{BlockPathResolver, BlockReader, FlowBlock};

use manifest_reader::flow_manifest_reader;

pub use manifest_reader::flow_manifest_reader::resolve_flow;

pub fn read_flow(
    flow_path: &Path, block_reader: &mut BlockReader, resolver: &mut BlockPathResolver,
) -> Result<FlowBlock> {
    FlowBlock::from_manifest(
        flow_manifest_reader::read_flow(flow_path)?,
        flow_path.to_owned(),
        block_reader,
        resolver.subflow(flow_path),
    )
}
