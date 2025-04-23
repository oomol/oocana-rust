use serde::Deserialize;

use crate::{extend_node_common_field, manifest::NodeInputFrom};

use super::common::{default_concurrency, NodeId};

extend_node_common_field!(SubflowNode {
    subflow: String,
    slots: Option<Vec<SubflowNodeSlots>>,
});
#[derive(Deserialize, Debug, Clone)]
pub struct SubflowNodeSlots {
    pub slot_node_id: NodeId,
    pub outputs_from: Vec<NodeInputFrom>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subflow_node() {
        let yaml = r#"
        subflow: example_subflow
        node_id: example_node
        inputs_from:
          - handle: input_handle
            value: null
        concurrency: 5
        ignore: false
        "#;

        let node: SubflowNode = serde_yaml::from_str(yaml).unwrap();

        // subflow value is not really usage value, just a placeholder
        assert_eq!(node.subflow, "example_subflow");
        assert_eq!(node.node_id, NodeId::from("example_node".to_owned()));
        assert_eq!(node.concurrency, 5);
        assert_eq!(node.ignore, false);
    }
}
