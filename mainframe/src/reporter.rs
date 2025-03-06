use async_trait::async_trait;
use flume::{Receiver, Sender};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use utils::output::OutputValue;

use crate::MessageData;
use job::{BlockInputs, BlockJobStackLevel, BlockJobStacks, JobId, SessionId};
use manifest_meta::JsonValue;

mod block_reporter;
mod flow_reporter;
pub use block_reporter::BlockReporterTx;
pub use flow_reporter::FlowReporterTx;

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ReporterMessage<'a> {
    SessionStarted {
        session_id: &'a str,
        create_at: u128,
        path: &'a str,
    },
    SessionFinished {
        session_id: &'a str,
        finish_at: u128,
        path: &'a str,
    },
    FlowStarted {
        session_id: &'a str,
        job_id: &'a str,
        flow_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        create_at: u128,
    },
    FlowBlockStarted {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        inputs: &'a Option<BlockInputs>,
        stacks: &'a Vec<BlockJobStackLevel>,
        create_at: u128,
    },
    FlowFinished {
        session_id: &'a str,
        job_id: &'a str,
        flow_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        error: &'a Option<String>,
        finish_at: u128,
    },
    FlowBlockFinished {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        error: &'a Option<String>,
        finish_at: u128,
    },
    // TODO: 应该永远不会触发
    FlowOutput {
        session_id: &'a str,
        job_id: &'a str,
        flow_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        output: Arc<OutputValue>,
        handle: &'a str,
    },
    FlowBlockOutput {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        output: Arc<OutputValue>,
        handle: &'a str,
    },
    BlockStarted {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        inputs: &'a Option<BlockInputs>,
        create_at: u128,
    },
    BlockFinished {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        error: &'a Option<String>,
        finish_at: u128,
    },
    BlockOutput {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        output: &'a JsonValue,
        handle: &'a str,
        done: bool,
    },
    BlockLog {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        log: &'a str,
        stdio: &'a str,
    },
    BlockError {
        session_id: &'a str,
        job_id: &'a str,
        block_path: &'a Option<String>,
        stacks: &'a Vec<BlockJobStackLevel>,
        error: &'a str,
    },
}

impl<'a> ReporterMessage<'a> {
    pub fn now() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    }
}

enum Command {
    Report(MessageData),
    Abort,
}

#[async_trait]
pub trait ReporterTxImpl {
    async fn send(&self, data: MessageData);
    async fn disconnect(&self);
}

pub trait ReporterRxImpl {
    fn event_loop(self) -> tokio::task::JoinHandle<()>;
}

#[derive(Clone)]
pub struct ReporterTx {
    session_id: SessionId,
    tx: Option<Sender<Command>>,
}

impl ReporterTx {
    pub fn session_started(&self, path: &str) {
        self.send(ReporterMessage::SessionStarted {
            session_id: &self.session_id,
            create_at: ReporterMessage::now(),
            path,
        });
    }

    pub fn session_finished(&self, path: &str) {
        self.send(ReporterMessage::SessionFinished {
            session_id: &self.session_id,
            finish_at: ReporterMessage::now(),
            path,
        });
    }

    pub fn send(&self, data: ReporterMessage) {
        let payload = serde_json::to_vec(&data).unwrap();
        if let Some(tx) = self.tx.as_ref() {
            tx.send(Command::Report(payload)).unwrap();
        } else {
            println!("[Reporter] {}", serde_json::to_string(&data).unwrap());
        }
    }

    pub fn block(
        &self, job_id: JobId, block_path: Option<String>, stacks: BlockJobStacks,
    ) -> BlockReporterTx {
        BlockReporterTx::new(job_id, block_path, stacks, self.clone())
    }

    pub fn flow(
        &self, job_id: JobId, flow_path: Option<String>, stacks: BlockJobStacks,
    ) -> FlowReporterTx {
        FlowReporterTx::new(job_id, flow_path, stacks, self.clone())
    }

    pub fn abort(&self) {
        if let Some(tx) = self.tx.as_ref() {
            tx.send(Command::Abort).unwrap()
        }
    }
}

pub struct ReporterRx<TT, TR>
where
    TT: ReporterTxImpl,
    TR: ReporterRxImpl,
{
    impl_tx: Option<TT>,
    impl_rx: Option<TR>,
    rx: Option<Receiver<Command>>,
}

impl<TT, TR> ReporterRx<TT, TR>
where
    TT: ReporterTxImpl + Send + 'static,
    TR: ReporterRxImpl + Send + 'static,
{
    pub fn event_loop(self) -> tokio::task::JoinHandle<()> {
        let Self {
            impl_rx,
            impl_tx,
            rx,
        } = self;

        if let Some(impl_rx) = impl_rx {
            impl_rx.event_loop();
        }

        tokio::spawn(async move {
            if let Some(rx) = rx {
                loop {
                    match rx.recv_async().await {
                        Ok(Command::Report(data)) => {
                            if let Some(impl_tx) = &impl_tx {
                                impl_tx.send(data).await;
                            }
                        }
                        Ok(Command::Abort) => {
                            if let Some(tx) = impl_tx {
                                tx.disconnect().await;
                            }
                            break;
                        }
                        Err(e) => {
                            eprintln!("Reporter event-loop breaks unexpectedly: {:?}", e);
                            break;
                        }
                    }
                }
            }
        })
    }
}

pub fn create<TT, TR>(
    session_id: SessionId, impl_tx: Option<TT>, impl_rx: Option<TR>,
) -> (ReporterTx, ReporterRx<TT, TR>)
where
    TT: ReporterTxImpl,
    TR: ReporterRxImpl,
{
    if impl_tx.is_some() {
        let (tx, rx) = flume::unbounded();
        (
            ReporterTx {
                session_id,
                tx: Some(tx),
            },
            ReporterRx {
                impl_tx,
                impl_rx,
                rx: Some(rx),
            },
        )
    } else {
        (
            ReporterTx {
                session_id,
                tx: None,
            },
            ReporterRx {
                impl_tx,
                impl_rx,
                rx: None,
            },
        )
    }
}
