use manifest_reader::{path_finder::BlockPathFinder, reader};
use std::path::Path;
use utils::error::Result;

use crate::{BlockResolver, SubflowBlock};

pub fn read_flow(
    flow_path: &Path,
    block_resolver: &mut BlockResolver,
    path_finder: &mut BlockPathFinder,
) -> Result<SubflowBlock> {
    SubflowBlock::from_manifest(
        reader::read_flow(flow_path)?,
        flow_path.to_owned(),
        block_resolver,
        path_finder.subflow(flow_path),
    )
}
