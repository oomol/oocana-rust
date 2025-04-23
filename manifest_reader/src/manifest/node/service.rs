use serde::Deserialize;

use crate::{extend_node_common_field, manifest::NodeInputFrom};

use super::NodeId;

extend_node_common_field!(ServiceNode { service: String });

fn default_concurrency() -> i32 {
    1
}
