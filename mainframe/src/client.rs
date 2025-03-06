use crate::{ClientMessage, MBlockInput, MBlockReady, ServerMessage};
use log::info;
use utils::error::Result;

pub struct Client {
    requester: zmq::Socket,
}

impl Client {
    pub fn new() -> Self {
        let context = zmq::Context::new();
        let requester = context.socket(zmq::REQ).unwrap();

        Client { requester }
    }

    pub fn connect(&self, address: &str) -> Result<()> {
        self.requester.connect(address)?;
        info!("Client Started");
        Ok(())
    }

    pub fn send(&self, msg: ClientMessage) -> Result<Option<ServerMessage>> {
        self.requester
            .send(serde_json::to_string(&msg)?.as_bytes(), 0)?;

        let mut msg_carrier = zmq::Message::new();

        self.requester.recv(&mut msg_carrier, 0)?;

        let m_recv = match msg_carrier.as_str() {
            None => None,
            Some(m) => serde_json::from_str::<ServerMessage>(m).ok(),
        };

        Ok(m_recv)
    }

    pub fn send_ready(&self, msg: MBlockReady) -> Result<MBlockInput> {
        match self.send(ClientMessage::BlockReady(msg.clone()))? {
            Some(ServerMessage::BlockInput(m)) => Ok(m),
            _ => Ok(MBlockInput {
                block_task_id: msg.block_task_id,
                pipeline_task_id: msg.pipeline_task_id,
                input: None,
                options: None,
            }),
        }
    }
}
