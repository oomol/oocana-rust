#[cfg(test)]
mod tests {
    use manifest_reader::reader::{read_flow_block, read_task_block};
    use std::path::PathBuf;
    use utils::error::Result;
    fn test_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
    }

    #[test]
    fn load_flow() -> Result<()> {
        let base_dir = test_directory();

        let flow_block = read_flow_block(&base_dir.join("flow.oo.yaml"))?;
        assert_eq!(flow_block.nodes[0].concurrency(), 3);

        Ok(())
    }

    #[test]
    fn load_block() -> Result<()> {
        let base_dir = test_directory();

        let flow_block = read_task_block(&base_dir.join("block.oo.yaml"))?;

        assert_eq!(flow_block.additional_inputs, true);
        assert_eq!(flow_block.additional_outputs, true);

        Ok(())
    }
}
