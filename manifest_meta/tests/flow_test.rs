use manifest_meta::{BlockResolver, HandleName, NodeId};
use manifest_reader::path_finder::BlockPathFinder;

use std::path::PathBuf;
use utils::error::Result;

#[test]
fn it_should_read_flow_block() -> Result<()> {
    let base_dir = dirname();
    let block_search_paths = Some(vec![dirname()]);
    let mut finder = BlockPathFinder::new(base_dir, block_search_paths);
    let mut block_reader = BlockResolver::new();
    let flow_block = block_reader.resolve_flow_block("flow-1", &mut finder)?;

    assert!(flow_block.path.ends_with("flow-1/subflow.oo.yaml"));

    assert!(flow_block.inputs_def.is_none());
    assert!(flow_block.outputs_def.is_none());

    assert_eq!(flow_block.flow_inputs_tos.len(), 0);
    assert_eq!(flow_block.flow_outputs_froms.len(), 0);

    let node1_id = NodeId::new("node1".to_owned());

    let node1 = flow_block.nodes.get(&node1_id).unwrap();
    assert_eq!(node1.node_id(), &node1_id);
    assert!(matches!(node1, manifest_meta::Node::Task(_)));

    let node2_id = NodeId::new("node2".to_owned());
    let handle_in1 = HandleName::new("in1".to_owned());
    let handle_out1 = HandleName::new("out1".to_owned());
    let handle_out2 = HandleName::new("out2".to_owned());

    let node2 = flow_block.nodes.get(&node2_id).unwrap();
    assert!(matches!(node2, manifest_meta::Node::Task(_)));
    if let manifest_meta::Node::Task(task_node) = node2 {
        let from_node1 = task_node.inputs.get(&handle_in1).unwrap();

        assert!(matches!(
            from_node1.from.as_ref().unwrap().first().unwrap(),
            manifest_meta::HandleSource::NodeOutput { .. }
        ));
        if let manifest_meta::HandleSource::NodeOutput {
            node_id,
            output_handle,
        } = from_node1.from.as_ref().unwrap().first().unwrap()
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
            node_input_handle,
        } = to_node3
        {
            assert_eq!(node_id, &NodeId::new("node3".to_owned()));
            assert_eq!(node_input_handle, &handle_in1);
        }
    }

    Ok(())
}

#[test]
fn it_should_read_subflow_block_with_inputs_def() -> Result<()> {
    let base_dir = dirname();
    let block_search_paths = Some(vec![dirname()]);
    let mut finder = BlockPathFinder::new(base_dir, block_search_paths);
    let mut block_reader = BlockResolver::new();
    let flow_block = block_reader.resolve_flow_block("flow-2", &mut finder)?;

    let handle_flow_in1 = HandleName::new("flow_in1".to_owned());
    let handle_in1 = HandleName::new("in1".to_owned());

    assert!(flow_block.path.ends_with("flow-2/subflow.oo.yaml"));

    assert_eq!(
        flow_block
            .inputs_def
            .as_ref()
            .unwrap()
            .get(&handle_flow_in1)
            .unwrap()
            .handle,
        handle_flow_in1
    );
    assert!(flow_block.outputs_def.is_none());

    assert_eq!(flow_block.flow_outputs_froms.len(), 0);

    let node1_id = NodeId::new("node1".to_owned());

    let to_node1 = flow_block
        .flow_inputs_tos
        .get(&handle_flow_in1)
        .unwrap()
        .first()
        .unwrap();
    assert!(matches!(
        to_node1,
        manifest_meta::HandleTo::ToNodeInput { .. }
    ));
    if let manifest_meta::HandleTo::ToNodeInput {
        node_id,
        node_input_handle,
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
            .from
            .as_ref()
            .unwrap()
            .first()
            .unwrap();

        assert!(matches!(
            from_subflow,
            manifest_meta::HandleSource::FlowInput { .. }
        ));
        if let manifest_meta::HandleSource::FlowInput { input_handle } = from_subflow {
            assert_eq!(input_handle, &handle_flow_in1);
        }
    }

    Ok(())
}

fn dirname() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/flow_test")
}
