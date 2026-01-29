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
pub use flow::{
    generate_runtime_handle_name, InjectionStore, InjectionTarget, MergeInputsValue, SubflowBlock,
};

mod condition;
pub use condition::ConditionBlock;

mod slot;
pub use slot::SlotBlock;

mod node;
pub use node::{
    HandleFrom, HandleSource, HandleTo, HandlesFroms, HandlesTos, InputDefPatchMap, Node,
    NodesHandlesTos, ServiceNode, Slot, SlotNode, SubflowNode, ValueState,
};

mod connections;

mod block_resolver;
pub use block_resolver::BlockResolver;
use utils::error::Result;
pub mod flow_resolver;

pub fn read_flow_or_block(
    block_name: &str,
    block_reader: &mut BlockResolver,
    path_finder: &mut BlockPathFinder,
) -> Result<Block> {
    use std::sync::RwLock;
    if let Ok(flow_path) = find_flow(block_name) {
        return flow_resolver::read_flow(&flow_path, block_reader, path_finder)
            .map(|flow| Block::Flow(Arc::new(RwLock::new(flow))));
    }

    block_reader.resolve_block(block_name, path_finder)
}
