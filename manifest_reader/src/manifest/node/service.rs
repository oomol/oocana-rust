use serde::Deserialize;

use crate::{extend_node_common_field, manifest::NodeInputFrom};

use super::common::{default_concurrency, NodeId};

extend_node_common_field!(ServiceNode { service: String });
