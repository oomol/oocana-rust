use manifest_reader::block_manifest_reader::{self, resolve_task_block};
use std::path::PathBuf;
use utils::error::Result;

#[test]
fn it_should_read_task_block() -> Result<()> {
    let base_dir = dirname().join("blocks1");
    let block_search_paths = vec![dirname().join("blocks1")];
    let task_path = resolve_task_block("task-blk1", &base_dir, &block_search_paths)?;
    let task_block = block_manifest_reader::read_task_block(&task_path)?;

    assert!(task_path.ends_with("task-blk1/block.oo.yaml"));
    assert_eq!(task_block.entry.bin, "cargo");

    Ok(())
}

fn dirname() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/block_manifest_reader_test")
}
