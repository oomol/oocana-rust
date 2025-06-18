use serde::Deserialize;

use crate::{extend_node_common_field, manifest::NodeInputFrom};

use super::common::{default_concurrency, NodeId};

extend_node_common_field!(ServiceNode { service: String });

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_node() {
        let yaml = r#"
        service: example_service
        node_id: example_node
        inputs_from:
          - handle: input_handle
            value: null
        concurrency: 5
        ignore: false
        "#;

        let node: ServiceNode = serde_yaml::from_str(yaml).unwrap();

        // service value is not really usage value, just a placeholder
        assert_eq!(node.service, "example_service");
        assert_eq!(node.node_id, NodeId::from("example_node".to_owned()));
        assert_eq!(node.concurrency, 5);
        assert!(!node.ignore);
    }
}
