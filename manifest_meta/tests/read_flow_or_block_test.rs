#[cfg(test)]
mod tests {

    use manifest_meta::{Block, BlockResolver, read_flow_or_block};
    use manifest_reader::path_finder::BlockPathFinder;

    use std::path::PathBuf;

    #[test]
    fn test_read_subflow() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(&base_dir, None);
        let mut block_reader = BlockResolver::new();

        let block = read_flow_or_block(
            base_dir
                .join("fixtures")
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
            block
        );
    }

    #[test]
    fn test_read_block() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(&base_dir, None);
        let mut block_reader = BlockResolver::new();

        let block = read_flow_or_block(
            base_dir
                .join("fixtures")
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
            block
        );
    }

    #[test]
    fn test_read_task() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(&base_dir, None);
        let mut block_reader = BlockResolver::new();

        let block = read_flow_or_block(
            base_dir
                .join("fixtures")
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
            block
        );
    }

    fn test_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
    }
}
