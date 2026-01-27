use manifest_reader::{path_finder::BlockPathFinder, reader};
use std::path::Path;
use utils::error::Result;

use crate::{BlockResolver, SubflowBlock};

pub fn read_flow(
    flow_path: &Path,
    block_resolver: &mut BlockResolver,
    path_finder: &BlockPathFinder,
) -> Result<SubflowBlock> {
    SubflowBlock::from_manifest(
        reader::read_flow(flow_path)?,
        flow_path.to_owned(),
        block_resolver,
        path_finder.subflow(flow_path),
        false, // use_lazy_loading = false
    )
}

/// Read a flow with lazy loading support for circular dependencies
pub fn read_flow_lazy(
    flow_path: &Path,
    block_resolver: &mut BlockResolver,
    path_finder: &BlockPathFinder,
) -> Result<SubflowBlock> {
    SubflowBlock::from_manifest(
        reader::read_flow(flow_path)?,
        flow_path.to_owned(),
        block_resolver,
        path_finder.subflow(flow_path),
        true, // use_lazy_loading = true
    )
}

pub fn read_slotflow(
    inputs_def: Option<manifest_reader::manifest::InputHandles>,
    slot_flow_path: &Path,
    block_resolver: &mut BlockResolver,
    path_finder: &BlockPathFinder,
) -> Result<SubflowBlock> {
    SubflowBlock::from_manifest(
        reader::read_slotflow(inputs_def, slot_flow_path)?,
        slot_flow_path.to_owned(),
        block_resolver,
        path_finder.subflow(slot_flow_path),
        false, // use_lazy_loading = false for slots (can be changed if needed)
    )
}
