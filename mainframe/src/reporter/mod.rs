use async_trait::async_trait;
use flume::{Receiver, Sender};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};
use utils::output::OutputValue;

use crate::MessageData;
use job::{BlockInputs, BlockJobStackLevel, BlockJobStacks, JobId, SessionId};
use manifest_meta::JsonValue;

mod block_reporter;
mod flow_reporter;
pub use block_reporter::BlockReporterTx;
pub use flow_reporter::FlowReporterTx;

#[derive(Serialize, Debug, Clone)]
pub struct SessionStarted<'a> {
    pub session_id: &'a str,
    pub create_at: u128,
    pub path: &'a str,
    pub partial: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct SessionFinished<'a> {
    pub session_id: &'a str,
    pub finish_at: u128,
    pub path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: &'a Option<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct FlowStarted<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    pub flow_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub create_at: u128,
}

#[derive(Serialize, Debug, Clone)]
pub struct FlowFinished<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    pub flow_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub error: &'a Option<String>,
    pub finish_at: u128,
}

#[derive(Serialize, Debug, Clone)]
pub struct FlowNodesWillRun<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    pub flow_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub start_nodes: &'a Vec<String>,
    pub mid_nodes: &'a Vec<String>,
    pub end_nodes: &'a Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct SubflowBlockStarted<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    pub block_path: &'a Option<String>,
    pub inputs: &'a Option<BlockInputs>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub create_at: u128,
}

#[derive(Serialize, Debug, Clone)]
pub struct SubflowBlockFinished<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub error: &'a Option<String>,
    pub finish_at: u128,
}

#[derive(Serialize, Debug, Clone)]
pub struct SubflowBlockOutput<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub output: Arc<OutputValue>,
    pub handle: &'a str,
}

#[derive(Serialize, Debug, Clone)]
pub struct BlockStarted<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub inputs: &'a Option<BlockInputs>,
    pub create_at: u128,
}

#[derive(Serialize, Debug, Clone)]
pub struct BlockFinished<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub error: &'a Option<String>,
    pub finish_at: u128,
}

#[derive(Serialize, Debug, Clone)]
pub struct BlockOutput<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub output: &'a JsonValue,
    pub handle: &'a str,
    pub done: bool,
}

#[derive(Serialize, Debug, Clone)]
pub struct BlockLog<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub log: &'a str,
    pub stdio: &'a str,
}

#[derive(Serialize, Debug, Clone)]
pub struct BlockError<'a> {
    pub session_id: &'a str,
    pub job_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_path: &'a Option<String>,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub error: &'a str,
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ReporterMessage<'a> {
    SessionStarted(SessionStarted<'a>),
    SessionFinished(SessionFinished<'a>),
    FlowStarted(FlowStarted<'a>),
    FlowFinished(FlowFinished<'a>),
    FlowNodesWillRun(FlowNodesWillRun<'a>),
    SubflowBlockStarted(SubflowBlockStarted<'a>),
    SubflowBlockFinished(SubflowBlockFinished<'a>),
    SubflowBlockOutput(SubflowBlockOutput<'a>),
    BlockStarted(BlockStarted<'a>),
    BlockFinished(BlockFinished<'a>),
    BlockOutput(BlockOutput<'a>),
    BlockLog(BlockLog<'a>),
    BlockError(BlockError<'a>),
    // TODO: BlockWarning
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
    pub fn session_started(&self, path: &str, partial: bool) {
        self.send(ReporterMessage::SessionStarted(SessionStarted {
            session_id: &self.session_id,
            create_at: ReporterMessage::now(),
            path,
            partial,
        }));
    }

    pub fn session_finished(&self, path: &str, err: &Option<String>) {
        self.send(ReporterMessage::SessionFinished(SessionFinished {
            session_id: &self.session_id,
            finish_at: ReporterMessage::now(),
            path,
            error: &err,
        }));
    }

    pub fn send(&self, data: ReporterMessage) {
        let payload = serde_json::to_vec(&data).unwrap();
        if let Some(tx) = self.tx.as_ref() {
            tx.send(Command::Report(payload)).unwrap();
        } else {
            info!("[Reporter] {}", serde_json::to_string(&data).unwrap());
        }
    }

    pub fn block(
        &self,
        job_id: JobId,
        block_path: Option<String>,
        stacks: BlockJobStacks,
    ) -> BlockReporterTx {
        BlockReporterTx::new(job_id, block_path, stacks, self.clone())
    }

    pub fn flow(
        &self,
        job_id: JobId,
        flow_path: Option<String>,
        stacks: BlockJobStacks,
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
                            warn!("Reporter event-loop breaks unexpectedly: {:?}", e);
                            break;
                        }
                    }
                }
            }
        })
    }
}

pub fn create<TT, TR>(
    session_id: SessionId,
    impl_tx: Option<TT>,
    impl_rx: Option<TR>,
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
