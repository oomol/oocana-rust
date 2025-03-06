use clap::Parser;
use job::{JobId, SessionId};
use mainframe::{
    worker::{self, WorkerRxHandle, WorkerTx},
    JsonValue,
};
use std::{collections::HashMap, net::SocketAddr};

use crate::args::Args;

pub async fn connect() -> (VocanaSDK, WorkerRxHandle) {
    let Args {
        address,
        session_id,
        job_id,
    } = Args::parse();

    let (impl_tx, impl_rx) = mainframe_mqtt::worker::connect(
        address.parse::<SocketAddr>().unwrap(),
        SessionId::new(session_id.to_owned()),
        JobId::new(job_id.to_owned()),
    )
    .await;

    let (tx, rx) = worker::create(
        SessionId::new(session_id.to_owned()),
        JobId::new(job_id.to_owned()),
        impl_tx,
        impl_rx,
    );

    let event_loop_handle = rx.event_loop();

    let inputs = tx.ready().await;

    (
        VocanaSDK {
            session_id,
            job_id,
            inputs,
            tx,
        },
        event_loop_handle,
    )
}

pub struct VocanaSDK {
    pub session_id: String,
    pub job_id: String,
    pub inputs: Option<HashMap<String, JsonValue>>,
    tx: WorkerTx,
}

impl VocanaSDK {
    pub fn output(&self, output: &JsonValue, handle: &str, done: bool) {
        self.tx.output(output, handle, done);
    }

    pub fn error(&self, error: &str) {
        self.tx.error(&error.to_string());
    }

    pub fn done(&self, error: Option<&str>) {
        self.tx.done(error);
    }
}
