pub mod reporter;
pub mod scheduler;
pub mod worker;

pub use serde_json::Value as JsonValue;

pub type MessageData = Vec<u8>;
