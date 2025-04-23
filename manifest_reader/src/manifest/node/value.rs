use serde::Deserialize;

use crate::manifest::InputHandle;

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct ValueNode {
    pub node_id: NodeId,
    pub values: Vec<InputHandle>,
    #[serde(default)]
    pub ignore: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_node() {
        let yaml = r#"
        node_id: example_node
        values:
          - handle: input_handle
            value: null
        ignore: false
        "#;

        let node: ValueNode = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(node.node_id, NodeId::from("example_node".to_owned()));
        assert_eq!(node.values.len(), 1);
        assert_eq!(node.ignore, false);
    }
}
