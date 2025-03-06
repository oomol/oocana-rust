use serde::{Deserialize, Serialize};

pub use serde_json::Value as JsonValue;

pub use serde_json::json;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MBlockReady {
    pub pipeline_task_id: String,
    pub block_task_id: String,
    pub block_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MBlockInput {
    pub pipeline_task_id: String,
    pub block_task_id: String,
    #[serde(default)]
    pub input: Option<JsonValue>,
    #[serde(default)]
    pub options: Option<JsonValue>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MBlockOutput {
    pub pipeline_task_id: String,
    pub block_task_id: String,
    pub block_id: String,
    pub done: bool,
    pub slot_id: String,
    pub output: JsonValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MBlockError {
    pub pipeline_task_id: String,
    pub block_task_id: String,
    pub error: JsonValue,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MBlockBlockDone {
    pub pipeline_task_id: String,
    pub block_task_id: String,
    pub block_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MBlockEnd {
    pub pipeline_task_id: String,
    pub block_task_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    BlockReady(MBlockReady),
    BlockOutput(MBlockOutput),
    BlockError(MBlockError),
    BlockDone(MBlockBlockDone),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    BlockInput(MBlockInput),
    // BlockEnd(MBlockBlockEnd),
}

mod client;
mod server;

pub use client::Client;
pub use server::Server;
