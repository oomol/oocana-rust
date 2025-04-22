use serde::{Deserialize, Serialize};

// TODO:
// #[macro_export]
// macro_rules! extend_node_common_field {
//     ($name:ident { $($field:ident : $type:ty),* $(,)? }) => {
//         #[derive(Deserialize, Debug, Clone)]
//         pub struct $name {
//             $(pub $field: $type,)*
//             pub node_id: NodeId,
//             pub timeout: Option<u64>,
//             pub inputs_from: Option<Vec<NodeInputFrom>>,
//             #[serde(default = "default_concurrency")]
//             pub concurrency: i32,
//             #[serde(default)]
//             pub ignore: bool,
//         }
//     };
// }

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    derive_more::Display,
    derive_more::From,
    derive_more::FromStr,
    derive_more::Deref,
    derive_more::Constructor,
    derive_more::Into,
)]
pub struct NodeId(pub(crate) String);
