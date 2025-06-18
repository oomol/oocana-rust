use serde::Deserialize;

use crate::manifest::{HandleName, InputHandle};

use super::NodeId;

#[derive(Deserialize, Debug, Clone)]
pub struct ValueNode {
    pub node_id: NodeId,
    pub values: Vec<InputHandle>,
    #[serde(default)]
    pub ignore: bool,
}

impl ValueNode {
    pub fn get_handle(&self, handle: &HandleName) -> Option<InputHandle> {
        self.values
            .iter()
            .find(|input_handle| &input_handle.handle == handle)
            .cloned()
    }
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
        assert!(!node.ignore);
    }
}
