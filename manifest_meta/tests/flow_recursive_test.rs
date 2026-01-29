#[cfg(test)]
mod tests {

    use manifest_meta::{BlockResolver, HandleName, NodeId};
    use manifest_reader::path_finder::BlockPathFinder;

    use std::path::PathBuf;
    use std::sync::Arc;

    /// Self-referencing subflow: the flow contains a subflow node that points to itself.
    /// Verifies the placeholder-based recursive flow resolution doesn't loop and produces
    /// correct structure.
    #[test]
    fn test_recursive_subflow() {
        let base_dir = test_directory();
        let mut finder = BlockPathFinder::new(base_dir, None);
        let mut block_reader = BlockResolver::new();

        let flow_block = block_reader
            .resolve_flow_block("recursive", &mut finder)
            .unwrap();
        let guard = flow_block.read().unwrap();

        assert!(guard.path.ends_with("recursive/subflow.oo.yaml"));

        // flow inputs_def: depth
        let depth_handle = HandleName::new("depth".to_owned());
        assert!(guard
            .inputs_def
            .as_ref()
            .unwrap()
            .contains_key(&depth_handle));

        // flow outputs_froms: result from recurse.result
        let result_handle = HandleName::new("result".to_owned());
        let output_froms = guard.flow_outputs_froms.get(&result_handle).unwrap();
        assert_eq!(output_froms.len(), 1);
        assert!(matches!(
            &output_froms[0],
            manifest_meta::HandleFrom::FromNodeOutput {
                node_id,
                output_handle,
            } if *node_id == NodeId::new("recurse".to_owned())
              && *output_handle == result_handle
        ));

        assert_eq!(guard.nodes.len(), 2);

        // flow_inputs_tos: depth → worker.in1
        let worker_id = NodeId::new("worker".to_owned());
        let in1_handle = HandleName::new("in1".to_owned());
        let tos = guard.flow_inputs_tos.get(&depth_handle).unwrap();
        assert_eq!(tos.len(), 1);
        assert!(matches!(
            &tos[0],
            manifest_meta::HandleTo::ToNodeInput { node_id, input_handle }
            if *node_id == worker_id && *input_handle == in1_handle
        ));

        // worker node: Task with in1 from flow input, out1 → recurse.depth
        {
            let worker_node = guard.nodes.get(&worker_id).unwrap();
            assert!(matches!(worker_node, manifest_meta::Node::Task(_)));
            if let manifest_meta::Node::Task(task) = worker_node {
                assert!(task.task.path.as_ref().unwrap().ends_with("task.oo.yaml"));

                // in1 source is flow input
                let in1_input = task.inputs.get(&in1_handle).unwrap();
                assert!(matches!(
                    in1_input.sources.as_ref().unwrap().first().unwrap(),
                    manifest_meta::HandleSource::FlowInput { input_handle }
                    if *input_handle == depth_handle
                ));

                // to: out1 → recurse.depth
                let out1_handle = HandleName::new("out1".to_owned());
                let to = task.to.as_ref().unwrap();
                let out1_tos = to.get(&out1_handle).unwrap();
                assert!(matches!(
                    &out1_tos[0],
                    manifest_meta::HandleTo::ToNodeInput { node_id, input_handle }
                    if *node_id == NodeId::new("recurse".to_owned())
                    && *input_handle == depth_handle
                ));
            }
        }

        // recurse node: SubflowNode that references the same flow (self-reference)
        {
            let recurse_id = NodeId::new("recurse".to_owned());
            let recurse_node = guard.nodes.get(&recurse_id).unwrap();
            assert!(matches!(recurse_node, manifest_meta::Node::Flow(_)));

            if let manifest_meta::Node::Flow(subflow_node) = recurse_node {
                // The recursive subflow Arc points to the exact same RwLock as the parent flow
                assert!(Arc::ptr_eq(&flow_block, &subflow_node.flow));

                // to: result → flow output
                let to = subflow_node.to.as_ref().unwrap();
                let result_tos = to.get(&result_handle).unwrap();
                assert!(matches!(
                    &result_tos[0],
                    manifest_meta::HandleTo::ToFlowOutput { output_handle }
                    if *output_handle == result_handle
                ));

                // Recursive node has empty inputs because the placeholder had no inputs_def
                // when the subflow node was processed. This is expected behavior.
                assert!(subflow_node.inputs.is_empty());
            }
        }
    }

    fn test_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }
}
