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
    block_reader: &mut BlockResolver,
    path_finder: &mut BlockPathFinder,
) -> Result<Block> {
    if let Ok(flow_path) = find_flow(block_name) {
        return flow_resolver::read_flow(&flow_path, block_reader, path_finder)
            .map(|flow| Block::Flow(Arc::new(flow)));
    }

    block_reader.resolve_block(block_name, path_finder)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use manifest_reader::path_finder::BlockPathFinder;

    #[test]
    fn test_read_subflow() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
        let mut finder = BlockPathFinder::new(&base_dir, None);
        let mut block_reader = BlockResolver::new();

        let block = read_flow_or_block(
            base_dir
                .join("flow_test")
                .join("basic")
                .join("subflow.oo.yaml")
                .to_str()
                .unwrap(),
            &mut block_reader,
            &mut finder,
        )
        .unwrap();

        assert!(
            matches!(block, Block::Flow(_)),
            "Expected a Flow block, found {:?}",
            block.variant_name()
        );
    }

    #[test]
    fn test_read_block() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
        let mut finder = BlockPathFinder::new(&base_dir, None);
        let mut block_reader = BlockResolver::new();

        let block = read_flow_or_block(
            base_dir
                .join("flow_test")
                .join("basic")
                .join("block.oo.yaml")
                .to_str()
                .unwrap(),
            &mut block_reader,
            &mut finder,
        )
        .unwrap();

        assert!(
            matches!(block, Block::Task(_)),
            "Expected a Task block, found {:?}",
            block.variant_name()
        );
    }

    #[test]
    fn test_read_task() {
        let base_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
        let mut finder = BlockPathFinder::new(&base_dir, None);
        let mut block_reader = BlockResolver::new();

        let block = read_flow_or_block(
            base_dir
                .join("flow_test")
                .join("task.oo.yaml")
                .to_str()
                .unwrap(),
            &mut block_reader,
            &mut finder,
        )
        .unwrap();

        assert!(
            matches!(block, Block::Task(_)),
            "Expected a Task block, found {:?}",
            block.variant_name()
        );
    }
}
