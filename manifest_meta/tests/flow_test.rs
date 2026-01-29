#[cfg(test)]
mod tests {

    use manifest_meta::{
        generate_runtime_handle_name, BlockResolver, HandleName, NodeId,
    };
    use manifest_reader::path_finder::BlockPathFinder;

    use std::path::PathBuf;

    #[test]
    fn test_basic_subflow() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(base_dir, None);
        let mut block_reader = BlockResolver::new();

        let flow_block = block_reader
            .resolve_flow_block("basic", &mut finder)
            .unwrap();
        let flow_block = flow_block.read().unwrap();

        let flow_input = HandleName::new("flow_in1".to_owned());
        let handle_in1 = HandleName::new("in1".to_owned());

        assert!(flow_block.path.ends_with("basic/subflow.oo.yaml"));

        assert_eq!(
            flow_block
                .inputs_def
                .as_ref()
                .unwrap()
                .get(&flow_input)
                .unwrap()
                .handle,
            flow_input
        );
        assert!(flow_block.outputs_def.is_none());

        assert_eq!(flow_block.flow_outputs_froms.len(), 0);

        let node1_id = NodeId::new("node1".to_owned());

        let to_node1 = flow_block
            .flow_inputs_tos
            .get(&flow_input)
            .unwrap()
            .first()
            .unwrap();
        assert!(matches!(
            to_node1,
            manifest_meta::HandleTo::ToNodeInput { .. }
        ));
        if let manifest_meta::HandleTo::ToNodeInput {
            node_id,
            input_handle: node_input_handle,
        } = to_node1
        {
            assert_eq!(node_id, &node1_id);
            assert_eq!(node_input_handle, &handle_in1);
        }

        let node1 = flow_block.nodes.get(&node1_id).unwrap();
        assert_eq!(node1.node_id(), &node1_id);
        assert!(matches!(node1, manifest_meta::Node::Task(_)));
        if let manifest_meta::Node::Task(task_node) = node1 {
            let from_subflow = &task_node
                .inputs
                .get(&handle_in1)
                .unwrap()
                .sources
                .as_ref()
                .unwrap()
                .first()
                .unwrap();

            assert!(matches!(
                from_subflow,
                manifest_meta::HandleSource::FlowInput { .. }
            ));
            if let manifest_meta::HandleSource::FlowInput { input_handle } = from_subflow {
                assert_eq!(input_handle, &flow_input);
            }
        }
    }

    #[test]
    fn test_additional_subflow() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(base_dir, None);
        let mut block_reader = BlockResolver::new();
        let flow_block = block_reader
            .resolve_flow_block("additional", &mut finder)
            .unwrap();
        let flow_block = flow_block.read().unwrap();

        assert!(flow_block.path.ends_with("additional/subflow.oo.yaml"));

        assert!(flow_block.inputs_def.is_none());
        assert!(flow_block.outputs_def.is_none());

        assert_eq!(flow_block.flow_inputs_tos.len(), 0);
        assert_eq!(flow_block.flow_outputs_froms.len(), 0);

        // node1
        let node1_id = NodeId::new("node1".to_owned());
        {
            let node1 = flow_block.nodes.get(&node1_id).unwrap();
            assert_eq!(node1.node_id(), &node1_id);
            assert!(matches!(node1, manifest_meta::Node::Task(_)));
        }

        // node2
        let node2_id = NodeId::new("node2".to_owned());
        {
            let handle_in1 = HandleName::new("in1".to_owned());
            let handle_out1 = HandleName::new("out1".to_owned());
            let handle_out2 = HandleName::new("out2".to_owned());

            let node2 = flow_block.nodes.get(&node2_id).unwrap();
            assert!(matches!(node2, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task_node) = node2 {
                let input_in1 = task_node.inputs.get(&handle_in1).unwrap();

                let definition = input_in1.def.clone();
                assert_eq!(definition.handle, handle_in1);
                assert_eq!(definition.nullable, Some(true));
                assert!(definition
                    .json_schema
                    .is_some_and(|schema| { schema.get("type").is_some_and(|t| t == "string") }));

                assert!(matches!(
                    input_in1.sources.as_ref().unwrap().first().unwrap(),
                    manifest_meta::HandleSource::NodeOutput { .. }
                ));
                if let manifest_meta::HandleSource::NodeOutput {
                    node_id,
                    output_handle,
                } = input_in1.sources.as_ref().unwrap().first().unwrap()
                {
                    assert_eq!(node_id, &node1_id);
                    assert_eq!(output_handle, &handle_out2);
                }

                let to_node3 = task_node
                    .to
                    .as_ref()
                    .unwrap()
                    .get(&handle_out1)
                    .unwrap()
                    .first()
                    .unwrap();

                assert!(matches!(
                    to_node3,
                    manifest_meta::HandleTo::ToNodeInput { .. }
                ));
                if let manifest_meta::HandleTo::ToNodeInput {
                    node_id,
                    input_handle: node_input_handle,
                } = to_node3
                {
                    assert_eq!(node_id, &NodeId::new("node3".to_owned()));
                    assert_eq!(node_input_handle, &handle_in1);
                }
            }
        }

        let node3 = flow_block
            .nodes
            .get(&NodeId::new("node3".to_owned()))
            .unwrap();
        // node3
        {
            assert!(matches!(node3, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task_node) = node3 {
                let additional_in_handle = HandleName::new("additional_in".to_owned());

                let node_additional_in = task_node.inputs.get(&additional_in_handle);
                assert!(node_additional_in.is_some());
                let additional_inputs = node_additional_in.unwrap();
                assert!(additional_inputs.def.is_additional);

                let block_additional_in = node3
                    .inputs_def()
                    .and_then(|def| def.get(&additional_in_handle).cloned());
                assert!(block_additional_in.is_some());
                assert!(block_additional_in.unwrap().is_additional);

                let additional_out_handle = HandleName::new("additional_out".to_owned());

                let block_additional_out = node3
                    .outputs_def()
                    .and_then(|def| def.get(&additional_out_handle).cloned());
                assert!(block_additional_out.is_some());
                assert!(block_additional_out.unwrap().is_additional);
            }
        }
    }

    #[test]
    fn test_serializable_var_subflow() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(base_dir, None);
        let mut block_reader = BlockResolver::new();

        let flow_block = block_reader
            .resolve_flow_block("serializable-var", &mut finder)
            .unwrap();
        let flow_block = flow_block.read().unwrap();

        assert!(flow_block
            .path
            .ends_with("serializable-var/subflow.oo.yaml"));

        let node1_id = NodeId::new("node1".to_owned());
        // node1
        {
            let node1 = flow_block.nodes.get(&node1_id).unwrap();
            assert!(matches!(node1, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task_node) = node1 {
                let output_handle = HandleName::new("out".to_owned());
                let output = task_node
                    .task
                    .outputs_def
                    .as_ref()
                    .unwrap()
                    .get(&output_handle)
                    .unwrap();
                assert!(output._serialize_for_cache);
            }
        }

        let node2_id = NodeId::new("node2".to_owned());
        // node2
        {
            let node2 = flow_block.nodes.get(&node2_id).unwrap();
            assert!(matches!(node2, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task_node) = node2 {
                let input_handle = HandleName::new("in".to_owned());
                let input = node2
                    .inputs_def()
                    .and_then(|def| def.get(&input_handle).cloned())
                    .unwrap();
                assert!(input._deserialize_from_cache);

                let node_input = task_node.inputs.get(&input_handle).unwrap();
                assert!(node_input.serialize_for_cache);
            }
        }

        let node3 = flow_block
            .nodes
            .get(&NodeId::new("node3".to_owned()))
            .unwrap();
        // node3
        {
            assert!(matches!(node3, manifest_meta::Node::Flow(_)));
            if let manifest_meta::Node::Flow(subflow_node) = node3 {
                let additional_input = subflow_node.inputs.get(&generate_runtime_handle_name(
                    "+slot#1",
                    &HandleName::new("input2".to_owned()),
                ));
                assert!(additional_input.is_some());
                let additional_input = additional_input.unwrap();
                assert!(additional_input.def.is_additional);
                assert_eq!(additional_input.serialize_for_cache, true);
                assert_eq!(additional_input.def._deserialize_from_cache, true);
            }
        }
    }

    fn test_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }
}
