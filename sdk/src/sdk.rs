use clap::Parser;
use mainframe::{Client, ClientMessage, JsonValue, MBlockBlockDone, MBlockOutput, MBlockReady};
use utils::error::Result;

use crate::args::Args;

pub use mainframe::json;

pub struct VocanaSDK {
    mainframe_client: Client,
    pub block_meta: Args,
    pub block_input: Option<JsonValue>,
    pub block_options: Option<JsonValue>,
}

impl VocanaSDK {
    pub fn new() -> VocanaSDK {
        let config = Args::parse();
        let mainframe_client = Client::new();

        VocanaSDK {
            block_meta: config,
            mainframe_client,
            block_input: None,
            block_options: None,
        }
    }

    pub fn connect(&mut self) -> Result<()> {
        self.mainframe_client.connect(&self.block_meta.address)?;

        let m_block_input = self.mainframe_client.send_ready(MBlockReady {
            pipeline_task_id: (&self.block_meta.pipeline_task_id).to_owned(),
            block_task_id: (&self.block_meta.block_task_id).to_owned(),
            block_id: (&self.block_meta.block_id).to_owned(),
        })?;

        self.block_input = m_block_input.input;
        self.block_options = m_block_input.options;

        Ok(())
    }

    pub fn output(&self, slot_id: &str, done: bool, data: JsonValue) -> Result<()> {
        self.mainframe_client
            .send(ClientMessage::BlockOutput(MBlockOutput {
                pipeline_task_id: (&self.block_meta.pipeline_task_id).to_owned(),
                block_task_id: (&self.block_meta.block_task_id).to_owned(),
                block_id: (&self.block_meta.block_id).to_owned(),
                slot_id: slot_id.to_owned(),
                done,
                output: data,
            }))?;

        Ok(())
    }

    pub fn done(&self) -> Result<()> {
        self.mainframe_client
            .send(ClientMessage::BlockDone(MBlockBlockDone {
                pipeline_task_id: (&self.block_meta.pipeline_task_id).to_owned(),
                block_task_id: (&self.block_meta.block_task_id).to_owned(),
                block_id: (&self.block_meta.block_id).to_owned(),
            }))?;

        Ok(())
    }
}
