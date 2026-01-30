#[cfg(test)]
mod tests {
    use manifest_reader::manifest::HandleName;
    use manifest_reader::reader::{
        read_flow_block, read_package, read_service, read_slotflow, read_task_block,
    };
    use std::path::PathBuf;
    use utils::error::Result;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn load_flow() -> Result<()> {
        let flow_block = read_flow_block(&fixtures_dir().join("flow.oo.yaml"))?;
        assert_eq!(flow_block.nodes[0].concurrency(), 3);

        Ok(())
    }

    #[test]
    fn load_block() -> Result<()> {
        let flow_block = read_task_block(&fixtures_dir().join("block.oo.yaml"))?;

        assert!(flow_block.additional_inputs);
        assert!(flow_block.additional_outputs);

        Ok(())
    }

    #[test]
    fn load_service() -> Result<()> {
        let service = read_service(&fixtures_dir().join("service.oo.yaml"))?;

        let executor = service.executor.expect("executor should exist");
        assert_eq!(executor.name, Some("python".to_string()));
        assert_eq!(executor.entry, Some("main.py".to_string()));
        assert_eq!(executor.function, Some("handler".to_string()));
        assert_eq!(executor.keep_alive, Some(30));

        let blocks = service.blocks.expect("blocks should exist");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "service_block_1");

        Ok(())
    }

    #[test]
    fn load_package() -> Result<()> {
        let package = read_package(fixtures_dir().join("package.oo.yaml"))?;

        assert_eq!(package.name, Some("test-package".to_string()));
        assert_eq!(package.version, Some("1.0.0".to_string()));

        let exports = package.exports.expect("exports should exist");
        assert_eq!(exports.get("main"), Some(&"./src/main.rs".to_string()));
        assert_eq!(exports.get("utils"), Some(&"./src/utils.rs".to_string()));

        let scripts = package.scripts.expect("scripts should exist");
        assert_eq!(scripts.bootstrap, Some("npm install".to_string()));

        let deps = package.dependencies.expect("dependencies should exist");
        assert_eq!(deps.get("lodash"), Some(&"4.17.21".to_string()));

        Ok(())
    }

    #[test]
    fn load_slotflow() -> Result<()> {
        use manifest_reader::manifest::{InputHandle, InputHandles};
        use std::collections::HashMap;

        let handle_name = HandleName::new("injected_input".to_string());
        let input_handle = InputHandle {
            handle: handle_name.clone(),
            description: Some("Injected input".to_string()),
            json_schema: None,
            kind: None,
            nullable: None,
            value: None,
            remember: false,
            is_additional: false,
            _deserialize_from_cache: false,
        };

        let mut inputs_map: InputHandles = HashMap::new();
        inputs_map.insert(handle_name.clone(), input_handle);

        let slotflow = read_slotflow(Some(inputs_map), &fixtures_dir().join("slotflow.oo.yaml"))?;

        assert_eq!(slotflow.description, Some("Test slotflow".to_string()));
        assert_eq!(slotflow.nodes.len(), 1);
        assert_eq!(slotflow.nodes[0].node_id().to_string(), "inner-task");

        // inputs_def should be replaced by the injected one
        let inputs = slotflow.inputs_def.expect("inputs_def should exist");
        assert_eq!(inputs.len(), 1);
        assert!(inputs.contains_key(&handle_name));

        let outputs_from = slotflow.outputs_from.expect("outputs_from should exist");
        assert_eq!(outputs_from.len(), 1);

        Ok(())
    }
}
