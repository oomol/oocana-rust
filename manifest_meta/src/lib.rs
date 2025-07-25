use std::sync::Arc;

use manifest_reader::path_finder::{find_flow, BlockPathFinder};
pub use manifest_reader::{
    manifest::{
        HandleName, InputHandle, NodeId, OutputHandle, ServiceExecutorOptions, TaskBlockExecutor,
    },
    JsonValue,
};

pub use manifest_reader::manifest::{InputHandles, OutputHandles};

mod block;
pub use block::Block;

mod service;
pub use service::{Service, ServiceBlock};

mod service_resolver;

mod scope;
pub use scope::BlockScope;

mod task;
pub use task::TaskBlock;

mod flow;
pub use flow::{InjectionStore, InjectionTarget, MergeInputsValue, SubflowBlock};

mod slot;
pub use slot::SlotBlock;

mod node;
pub use node::{
    HandleFrom, HandleSource, HandleTo, HandlesFroms, HandlesTos, InputDefPatchMap, Node,
    NodesHandlesTos, ServiceNode, Slot, SlotNode, SubflowNode,
};

mod connections;

mod block_resolver;
pub use block_resolver::BlockResolver;
use utils::error::Result;
pub mod flow_resolver;

pub fn read_flow_or_block(
    block_name: &str,
    mut block_reader: BlockResolver,
    mut path_finder: BlockPathFinder,
) -> Result<Block> {
    if let Ok(flow_path) = find_flow(block_name) {
        return flow_resolver::read_flow(&flow_path, &mut block_reader, &mut path_finder)
            .map(|flow| Block::Flow(Arc::new(flow)));
    }

    block_reader.resolve_block(block_name, &mut path_finder)
}
