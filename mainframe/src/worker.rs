use std::collections::HashMap;

use crate::{scheduler, MessageData};
use async_trait::async_trait;
use flume::{Receiver, Sender};
use job::{JobId, SessionId};
use manifest_meta::{HandleName, JsonValue};
use tokio::sync::oneshot;

type BlockInputsDeserialize = HashMap<String, JsonValue>;

#[derive(serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MessageSerialize<'a> {
    BlockReady {
        session_id: &'a str,
        job_id: &'a str,
    },
    BlockOutput {
        session_id: &'a str,
        job_id: &'a str,
        done: bool,
        handle: &'a str,
        output: &'a JsonValue,
    },
    BlockError {
        session_id: &'a str,
        job_id: &'a str,
        error: &'a str,
    },
    BlockFinished {
        session_id: &'a str,
        job_id: &'a str,
        error: Option<&'a str>,
    },
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MessageDeserialize {
    BlockReady {
        session_id: SessionId,
        job_id: JobId,
    },
    BlockOutput {
        session_id: SessionId,
        job_id: JobId,
        done: bool,
        handle: HandleName,
        output: JsonValue,
    },
    BlockError {
        session_id: SessionId,
        job_id: JobId,
        error: String,
    },
    BlockFinished {
        session_id: SessionId,
        job_id: JobId,
        error: Option<String>,
    },
}

impl MessageDeserialize {
    pub fn session_id(&self) -> &SessionId {
        match self {
            MessageDeserialize::BlockReady { session_id, .. } => session_id,
            MessageDeserialize::BlockOutput { session_id, .. } => session_id,
            MessageDeserialize::BlockError { session_id, .. } => session_id,
            MessageDeserialize::BlockFinished { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> &JobId {
        match self {
            MessageDeserialize::BlockReady { job_id, .. } => job_id,
            MessageDeserialize::BlockOutput { job_id, .. } => job_id,
            MessageDeserialize::BlockError { job_id, .. } => job_id,
            MessageDeserialize::BlockFinished { job_id, .. } => job_id,
        }
    }
}

#[async_trait]
pub trait WorkerTxImpl {
    async fn send(&self, data: MessageData);
}

#[async_trait]
pub trait WorkerRxImpl {
    async fn recv(&mut self) -> MessageData;
}

enum Command {
    Ready(MessageData, oneshot::Sender<Option<BlockInputsDeserialize>>),
    SendMessage(MessageData, bool),
    ReceiveMessage(MessageData),
}

#[derive(Debug, Clone)]
pub struct WorkerTx {
    session_id: SessionId,
    job_id: JobId,
    tx: Sender<Command>,
}

impl WorkerTx {
    pub async fn ready(&self) -> Option<BlockInputsDeserialize> {
        let data = serde_json::to_vec(&MessageSerialize::BlockReady {
            session_id: &self.session_id,
            job_id: &self.job_id,
        })
        .unwrap();
        let (tx, rx) = oneshot::channel::<Option<BlockInputsDeserialize>>();
        self.tx.send(Command::Ready(data, tx)).unwrap();
        rx.await.unwrap()
    }

    pub fn output(&self, output: &JsonValue, handle: &str, done: bool) {
        self.send(
            MessageSerialize::BlockOutput {
                session_id: &self.session_id,
                job_id: &self.job_id,
                done,
                handle,
                output,
            },
            false,
        );
        if done {
            self.done(None);
        }
    }

    pub fn error(&self, error: &String) {
        self.send(
            MessageSerialize::BlockError {
                session_id: &self.session_id,
                job_id: &self.job_id,
                error,
            },
            false,
        );
    }

    pub fn done(&self, error: Option<&str>) {
        self.send(
            MessageSerialize::BlockFinished {
                session_id: &self.session_id,
                job_id: &self.job_id,
                error,
            },
            true,
        );
    }

    fn send(&self, message: MessageSerialize, finish: bool) {
        let data = serde_json::to_vec(&message).unwrap();
        self.tx.send(Command::SendMessage(data, finish)).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct WorkerRx<TT, TR>
where
    TT: WorkerTxImpl,
    TR: WorkerRxImpl,
{
    impl_tx: TT,
    impl_rx: TR,
    session_id: SessionId,
    job_id: JobId,
    tx: Sender<Command>,
    rx: Receiver<Command>,
}

impl<TT, TR> WorkerRx<TT, TR>
where
    TT: WorkerTxImpl + Send + 'static,
    TR: WorkerRxImpl + Send + 'static,
{
    pub fn event_loop(self) -> WorkerRxHandle {
        let Self {
            tx,
            rx,
            session_id,
            job_id,
            impl_tx,
            mut impl_rx,
        } = self;

        let command_handle = tokio::spawn(async move {
            let mut inputs_callback: Option<oneshot::Sender<Option<BlockInputsDeserialize>>> = None;

            loop {
                match rx.recv_async().await {
                    Ok(Command::Ready(data, tx)) => {
                        debug_assert!(&inputs_callback.is_none());

                        _ = inputs_callback.insert(tx);
                        impl_tx.send(data).await;
                    }
                    Ok(Command::SendMessage(data, done)) => {
                        impl_tx.send(data).await;
                        if done {
                            break;
                        }
                    }
                    Ok(Command::ReceiveMessage(data)) => {
                        if let Some(msg) = parse_scheduler_message(data, &session_id, &job_id) {
                            match msg {
                                scheduler::MessageDeserialize::BlockInputs { inputs, .. } => {
                                    debug_assert!(&inputs_callback.is_some());

                                    if let Some(callback) = inputs_callback.take() {
                                        callback.send(inputs).unwrap();
                                    }
                                }
                                scheduler::MessageDeserialize::ExecuteBlock { .. } => {}
                                scheduler::MessageDeserialize::ExecuteAppletBlock { .. } => {}
                            };
                        }
                    }
                    Err(e) => {
                        eprintln!("Worker event-loop breaks unexpectedly: {:?}", e);
                        break;
                    }
                }
            }
        });

        let impl_rx_handle = tokio::spawn(async move {
            loop {
                let data = impl_rx.recv().await;
                tx.send(Command::ReceiveMessage(data)).unwrap();
            }
        });

        WorkerRxHandle(command_handle, impl_rx_handle)
    }
}

pub struct WorkerRxHandle(tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>);

impl WorkerRxHandle {
    pub async fn wait(self) {
        _ = self.0.await;
        _ = self.1.await;
    }

    pub fn abort(&self) {
        self.0.abort();
        self.1.abort();
    }

    pub fn abort_handle(&self) -> WorkerAbortHandle {
        WorkerAbortHandle(self.0.abort_handle(), self.1.abort_handle())
    }
}

pub struct WorkerAbortHandle(tokio::task::AbortHandle, tokio::task::AbortHandle);

impl WorkerAbortHandle {
    pub fn abort(&self) {
        self.0.abort();
        self.1.abort();
    }
}

fn parse_scheduler_message(
    data: MessageData, session_id: &str, job_id: &str,
) -> Option<scheduler::MessageDeserialize> {
    match serde_json::from_slice::<scheduler::MessageDeserialize>(&data) {
        Ok(msg) => {
            if msg.session_id() == session_id && msg.job_id() == job_id {
                Some(msg)
            } else {
                None
            }
        }
        Err(e) => {
            eprintln!(
                "Incorrect message sending to worker. session_id: {:?} error: {:?}",
                session_id, e
            );
            None
        }
    }
}

pub fn create<TT, TR>(
    session_id: SessionId, job_id: JobId, impl_tx: TT, impl_rx: TR,
) -> (WorkerTx, WorkerRx<TT, TR>)
where
    TT: WorkerTxImpl,
    TR: WorkerRxImpl,
{
    let (tx, rx) = flume::unbounded();
    (
        WorkerTx {
            tx: tx.clone(),
            session_id: session_id.to_owned(),
            job_id: job_id.clone(),
        },
        WorkerRx {
            impl_tx,
            impl_rx,
            session_id,
            job_id,
            tx,
            rx,
        },
    )
}
