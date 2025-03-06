use crate::{ClientMessage, ServerMessage};
use log::info;
use utils::error::Result;

pub struct Server {
    responder: zmq::Socket,
}

impl Server {
    pub fn new() -> Self {
        let context = zmq::Context::new();
        let responder = context.socket(zmq::REP).unwrap();

        Server { responder }
    }

    pub fn bind(&self, address: &str) -> Result<()> {
        self.responder.bind(address)?;

        info!("Server Started");

        Ok(())
    }

    pub fn on_msg<F>(&self, mut handle_msg: F) -> Result<()>
    where
        F: FnMut(ClientMessage) -> Option<ServerMessage>,
    {
        let mut msg = zmq::Message::new();

        loop {
            self.responder.recv(&mut msg, 0)?;

            match msg.as_str() {
                None => {
                    self.responder.send("", 0)?;
                }
                Some(msg_str) => {
                    let recv_msg: ClientMessage = serde_json::from_str(msg_str)?;

                    match handle_msg(recv_msg) {
                        None => {
                            self.responder.send("", 0)?;
                        }
                        Some(response_msg) => {
                            self.responder
                                .send(serde_json::to_string(&response_msg)?.as_bytes(), 0)?;
                        }
                    }
                }
            }
        }
    }
}
