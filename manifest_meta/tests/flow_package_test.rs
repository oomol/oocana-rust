#[cfg(test)]
mod tests {

    use manifest_meta::{BlockResolver, HandleName, JsonValue, NodeId, ValueState};
    use manifest_reader::path_finder::BlockPathFinder;

    use std::path::PathBuf;

    /// Flow that references task blocks from an external package via `pkg::task` syntax.
    /// Verifies package-based task resolution, flow input/output wiring, and value propagation.
    #[test]
    fn test_package_task_flow() {
        let base_dir = test_directory();
        let packages_dir = base_dir.join("packages");
        let mut finder = BlockPathFinder::new(base_dir, Some(vec![packages_dir]));
        let mut block_reader = BlockResolver::new();

        let flow_block = block_reader
            .resolve_flow_block("package-task", &mut finder)
            .unwrap();
        let flow_block = flow_block.read().unwrap();

        assert!(flow_block.path.ends_with("package-task/subflow.oo.yaml"));

        // flow inputs_def: user_name
        let user_name = HandleName::new("user_name".to_owned());
        assert!(flow_block
            .inputs_def
            .as_ref()
            .unwrap()
            .contains_key(&user_name));

        // flow outputs_froms: output_message comes from greet.message
        let output_message = HandleName::new("output_message".to_owned());
        let output_froms = flow_block.flow_outputs_froms.get(&output_message).unwrap();
        assert_eq!(output_froms.len(), 1);
        assert!(matches!(
            &output_froms[0],
            manifest_meta::HandleFrom::FromNodeOutput {
                node_id,
                output_handle,
            } if *node_id == NodeId::new("greet".to_owned())
              && *output_handle == HandleName::new("message".to_owned())
        ));

        // flow_inputs_tos: user_name → greet.name
        let greet_id = NodeId::new("greet".to_owned());
        let name_handle = HandleName::new("name".to_owned());
        let tos = flow_block.flow_inputs_tos.get(&user_name).unwrap();
        assert_eq!(tos.len(), 1);
        assert!(matches!(
            &tos[0],
            manifest_meta::HandleTo::ToNodeInput { node_id, input_handle }
            if *node_id == greet_id && *input_handle == name_handle
        ));

        assert_eq!(flow_block.nodes.len(), 2);

        // greet node: task from package, input from flow
        {
            let greet_node = flow_block.nodes.get(&greet_id).unwrap();
            assert!(matches!(greet_node, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task) = greet_node {
                // task resolved from package should have a path ending in task.oo.yaml
                assert!(task
                    .task
                    .path
                    .as_ref()
                    .unwrap()
                    .ends_with("tasks/greeting/task.oo.yaml"));

                // task should have package_path pointing to test-pkg-1.0.0
                assert!(task
                    .task
                    .package_path
                    .as_ref()
                    .unwrap()
                    .ends_with("test-pkg-1.0.0"));

                // name input has FlowInput source
                let name_input = task.inputs.get(&name_handle).unwrap();
                let source = name_input.sources.as_ref().unwrap().first().unwrap();
                assert!(matches!(
                    source,
                    manifest_meta::HandleSource::FlowInput { input_handle }
                    if *input_handle == user_name
                ));

                // task has message output
                let message_handle = HandleName::new("message".to_owned());
                assert!(task
                    .task
                    .outputs_def
                    .as_ref()
                    .unwrap()
                    .contains_key(&message_handle));

                // greet node has to connection: message → flow output
                let to = task.to.as_ref().unwrap();
                let message_tos = to.get(&message_handle).unwrap();
                assert!(matches!(
                    &message_tos[0],
                    manifest_meta::HandleTo::ToFlowOutput { output_handle }
                    if *output_handle == output_message
                ));
            }
        }

        // process node: task from package, input has explicit value
        {
            let process_id = NodeId::new("process".to_owned());
            let process_node = flow_block.nodes.get(&process_id).unwrap();
            assert!(matches!(process_node, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task) = process_node {
                assert!(task
                    .task
                    .path
                    .as_ref()
                    .unwrap()
                    .ends_with("tasks/transform/task.oo.yaml"));

                let text_handle = HandleName::new("text".to_owned());
                let text_input = task.inputs.get(&text_handle).unwrap();

                // text input has explicit value "hello world"
                assert!(text_input.value.is_provided());
                assert_eq!(
                    text_input.value,
                    ValueState::Value(JsonValue::String(
                        "hello world".to_string()
                    ))
                );

                // text input has no connection source (value-only)
                assert!(text_input.sources.is_none());
            }
        }
    }

    /// Flow that references a subflow from an external package. The package subflow
    /// internally uses `self::` task references. Verifies nested package resolution
    /// and SubflowNode creation.
    #[test]
    fn test_package_subflow_flow() {
        let base_dir = test_directory();
        let packages_dir = base_dir.join("packages");
        let mut finder = BlockPathFinder::new(base_dir, Some(vec![packages_dir]));
        let mut block_reader = BlockResolver::new();

        let flow_block = block_reader
            .resolve_flow_block("package-flow", &mut finder)
            .unwrap();
        let flow_block = flow_block.read().unwrap();

        assert!(flow_block.path.ends_with("package-flow/subflow.oo.yaml"));

        // flow inputs_def: name
        let name_handle = HandleName::new("name".to_owned());
        assert!(flow_block
            .inputs_def
            .as_ref()
            .unwrap()
            .contains_key(&name_handle));

        // flow outputs_froms: result from pipeline.final_result
        let result_handle = HandleName::new("result".to_owned());
        let output_froms = flow_block.flow_outputs_froms.get(&result_handle).unwrap();
        assert_eq!(output_froms.len(), 1);
        assert!(matches!(
            &output_froms[0],
            manifest_meta::HandleFrom::FromNodeOutput {
                node_id,
                output_handle,
            } if *node_id == NodeId::new("pipeline".to_owned())
              && *output_handle == HandleName::new("final_result".to_owned())
        ));

        assert_eq!(flow_block.nodes.len(), 1);

        // pipeline node is a SubflowNode
        let pipeline_id = NodeId::new("pipeline".to_owned());
        let pipeline_node = flow_block.nodes.get(&pipeline_id).unwrap();
        assert!(matches!(pipeline_node, manifest_meta::Node::Flow(_)));

        if let manifest_meta::Node::Flow(subflow_node) = pipeline_node {
            // input_name has FlowInput source from name
            let input_name_handle = HandleName::new("input_name".to_owned());
            let input = subflow_node.inputs.get(&input_name_handle).unwrap();
            let source = input.sources.as_ref().unwrap().first().unwrap();
            assert!(matches!(
                source,
                manifest_meta::HandleSource::FlowInput { input_handle }
                if *input_handle == name_handle
            ));

            // Verify the nested subflow was resolved correctly
            let nested_flow = subflow_node.flow.read().unwrap();
            assert!(nested_flow
                .path
                .ends_with("subflows/pipeline/subflow.oo.yaml"));

            // nested subflow has 2 nodes (greeting_node and transform_node)
            assert_eq!(nested_flow.nodes.len(), 2);

            // greeting_node is a Task resolved via self::greeting
            let greeting_id = NodeId::new("greeting_node".to_owned());
            let greeting_node = nested_flow.nodes.get(&greeting_id).unwrap();
            assert!(matches!(greeting_node, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task) = greeting_node {
                assert!(task
                    .task
                    .path
                    .as_ref()
                    .unwrap()
                    .ends_with("tasks/greeting/task.oo.yaml"));
                assert!(task
                    .task
                    .package_path
                    .as_ref()
                    .unwrap()
                    .ends_with("test-pkg-1.0.0"));
            }

            // transform_node is a Task resolved via self::transform
            let transform_id = NodeId::new("transform_node".to_owned());
            let transform_node = nested_flow.nodes.get(&transform_id).unwrap();
            assert!(matches!(transform_node, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task) = transform_node {
                assert!(task
                    .task
                    .path
                    .as_ref()
                    .unwrap()
                    .ends_with("tasks/transform/task.oo.yaml"));

                // transform_node.text input comes from greeting_node.message
                let text_handle = HandleName::new("text".to_owned());
                let text_input = task.inputs.get(&text_handle).unwrap();
                let source = text_input.sources.as_ref().unwrap().first().unwrap();
                assert!(matches!(
                    source,
                    manifest_meta::HandleSource::NodeOutput { node_id, output_handle }
                    if *node_id == greeting_id
                    && *output_handle == HandleName::new("message".to_owned())
                ));
            }

            // nested subflow has flow outputs: final_result from transform_node.result
            let final_result = HandleName::new("final_result".to_owned());
            let nested_output_froms = nested_flow.flow_outputs_froms.get(&final_result).unwrap();
            assert!(matches!(
                &nested_output_froms[0],
                manifest_meta::HandleFrom::FromNodeOutput {
                    node_id,
                    output_handle,
                } if *node_id == transform_id
                  && *output_handle == HandleName::new("result".to_owned())
            ));

            // nested subflow has package_path set to the package root
            assert!(nested_flow
                .package_path
                .as_ref()
                .unwrap()
                .ends_with("test-pkg-1.0.0"));
        }
    }

    fn test_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/flow_test")
    }
}
