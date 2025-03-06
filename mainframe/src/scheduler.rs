use std::collections::HashMap;

use async_trait::async_trait;
use flume::{Receiver, Sender};

use job::{BlockInputs, BlockJobStackLevel, JobId, SessionId};
use manifest_meta::JsonValue;

use crate::{worker, MessageData};

#[derive(serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MessageSerialize<'a> {
    BlockInputs {
        session_id: &'a SessionId,
        job_id: &'a JobId,
        stacks: &'a Vec<BlockJobStackLevel>,
        block_path: &'a Option<String>,
        inputs: Option<&'a BlockInputs>,
    },
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MessageDeserialize {
    BlockInputs {
        session_id: String,
        job_id: String,
        stacks: Vec<BlockJobStackLevel>,
        block_path: Option<String>,
        inputs: Option<HashMap<String, JsonValue>>,
    },
}

impl MessageSerialize<'_> {
    pub fn session_id(&self) -> &SessionId {
        match self {
            MessageSerialize::BlockInputs { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> &JobId {
        match self {
            MessageSerialize::BlockInputs { job_id, .. } => job_id,
        }
    }
}

impl MessageDeserialize {
    pub fn session_id(&self) -> &str {
        match self {
            MessageDeserialize::BlockInputs { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> &str {
        match self {
            MessageDeserialize::BlockInputs { job_id, .. } => job_id,
        }
    }
}

#[async_trait]
pub trait SchedulerTxImpl {
    async fn send_inputs(&self, job_id: &JobId, data: MessageData);
    async fn disconnect(&self);
}

#[async_trait]
pub trait SchedulerRxImpl {
    async fn recv(&mut self) -> MessageData;
}

enum Command {
    RegisterSubscriber(JobId, Sender<worker::MessageDeserialize>),
    UnregisterSubscriber(JobId),
    SendInputs((JobId, MessageData)),
    ReceiveMessage(MessageData),
    Abort,
}

#[derive(Debug, Clone)]
pub struct SchedulerTx {
    session_id: SessionId,
    tx: Sender<Command>,
}

impl SchedulerTx {
    pub fn send_inputs(
        &self, job_id: JobId, block_path: &Option<String>, stacks: &Vec<BlockJobStackLevel>,
        inputs: Option<&BlockInputs>,
    ) {
        let data = serde_json::to_vec(&MessageSerialize::BlockInputs {
            session_id: &self.session_id,
            job_id: &job_id,
            inputs,
            stacks,
            block_path,
        })
        .unwrap();
        self.tx.send(Command::SendInputs((job_id, data))).unwrap()
    }

    pub fn register_subscriber(&self, job_id: JobId, sender: Sender<worker::MessageDeserialize>) {
        self.tx
            .send(Command::RegisterSubscriber(job_id, sender))
            .unwrap()
    }

    pub fn unregister_subscriber(&self, job_id: JobId) {
        self.tx.send(Command::UnregisterSubscriber(job_id)).unwrap()
    }

    pub fn abort(&self) {
        self.tx.send(Command::Abort).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerRx<TT, TR>
where
    TT: SchedulerTxImpl,
    TR: SchedulerRxImpl,
{
    impl_tx: TT,
    impl_rx: TR,
    session_id: SessionId,
    tx: Sender<Command>,
    rx: Receiver<Command>,
}

impl<TT, TR> SchedulerRx<TT, TR>
where
    TT: SchedulerTxImpl + Send + 'static,
    TR: SchedulerRxImpl + Send + 'static,
{
    pub fn event_loop(self) -> tokio::task::JoinHandle<()> {
        let mut subscribers = HashMap::new();
        let Self {
            tx,
            rx,
            session_id,
            impl_tx,
            mut impl_rx,
        } = self;

        tokio::spawn(async move {
            loop {
                let data = impl_rx.recv().await;
                tx.send(Command::ReceiveMessage(data)).unwrap();
            }
        });

        tokio::spawn(async move {
            loop {
                match rx.recv_async().await {
                    Ok(Command::RegisterSubscriber(job_id, sender)) => {
                        debug_assert!(subscribers.get(&job_id).is_none());
                        subscribers.insert(job_id, sender);
                    }
                    Ok(Command::UnregisterSubscriber(job_id)) => {
                        subscribers.remove(&job_id);
                    }
                    Ok(Command::SendInputs((job_id, data))) => {
                        impl_tx.send_inputs(&job_id, data).await;
                    }
                    Ok(Command::ReceiveMessage(data)) => {
                        if let Some(msg) = parse_worker_message(data, &session_id) {
                            if let Some(sender) = subscribers.get(msg.job_id()) {
                                sender.send(msg).unwrap();
                            }
                        }
                    }
                    Ok(Command::Abort) => {
                        impl_tx.disconnect().await;
                        break;
                    }
                    Err(e) => {
                        eprintln!("Scheduler event-loop breaks unexpectedly: {:?}", e);
                        break;
                    }
                }
            }
        })
    }
}

fn parse_worker_message(
    data: MessageData, session_id: &SessionId,
) -> Option<worker::MessageDeserialize> {
    match serde_json::from_slice::<worker::MessageDeserialize>(&data) {
        Ok(msg) => {
            if msg.session_id() == session_id {
                Some(msg)
            } else {
                None
            }
        }
        Err(e) => {
            eprintln!(
                "Incorrect message sending to scheduler. session_id: {:?} error: {:?}",
                session_id, e
            );
            None
        }
    }
}

pub fn create<TT, TR>(
    session_id: SessionId, impl_tx: TT, impl_rx: TR,
) -> (SchedulerTx, SchedulerRx<TT, TR>)
where
    TT: SchedulerTxImpl,
    TR: SchedulerRxImpl,
{
    let (tx, rx) = flume::unbounded();
    (
        SchedulerTx {
            tx: tx.clone(),
            session_id: session_id.to_owned(),
        },
        SchedulerRx {
            impl_tx,
            impl_rx,
            session_id,
            tx,
            rx,
        },
    )
}
