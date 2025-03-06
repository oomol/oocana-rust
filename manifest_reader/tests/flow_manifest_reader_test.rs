use manifest_reader::{path_finder::find_flow, reader::read_flow_block};
use std::path::PathBuf;
use utils::error::Result;

fn dirname() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn it_should_read_task_block() -> Result<()> {
    let base_dir = dirname();

    let flow_path = find_flow(base_dir.to_str().unwrap())?;

    let flow_block = read_flow_block(&flow_path)?;

    assert_eq!(flow_block.nodes[0].concurrency(), 3);

    Ok(())
}
