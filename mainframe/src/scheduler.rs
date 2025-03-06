use std::collections::HashMap;

use async_trait::async_trait;
use flume::{Receiver, Sender};

use job::{BlockInputs, BlockJobStackLevel, JobId, SessionId};
use manifest_meta::{
    AppletExecutorOptions, InputHandles, JsonValue, OutputHandles, TaskBlockExecutor,
};
use utils::output::DropSender;

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
        inputs_def: &'a Option<InputHandles>,
    },
    ExecuteBlock {
        executor_name: &'a str,
        session_id: &'a SessionId,
        job_id: &'a JobId,
        stacks: &'a Vec<BlockJobStackLevel>,
        dir: &'a str,
        executor: &'a TaskBlockExecutor,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: &'a Option<OutputHandles>,
    },
    ExecuteAppletBlock {
        session_id: &'a SessionId,
        job_id: &'a JobId,
        stacks: &'a Vec<BlockJobStackLevel>,
        executor_name: &'a str,
        block_name: &'a str,
        dir: &'a str,
        applet_executor: &'a AppletExecutorOptions,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: &'a Option<OutputHandles>,
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
    ExecuteBlock {
        executor_name: String,
        session_id: String,
        job_id: String,
        stacks: Vec<BlockJobStackLevel>,
        envs: HashMap<String, String>,
        dir: String,
    },
    ExecuteAppletBlock {
        executor_name: String,
        session_id: String,
        job_id: String,
        dir: String,
        applet_executor: AppletExecutorOptions,
    },
}

impl MessageSerialize<'_> {
    pub fn session_id(&self) -> &SessionId {
        match self {
            MessageSerialize::BlockInputs { session_id, .. } => session_id,
            MessageSerialize::ExecuteBlock { session_id, .. } => session_id,
            MessageSerialize::ExecuteAppletBlock { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> &JobId {
        match self {
            MessageSerialize::BlockInputs { job_id, .. } => job_id,
            MessageSerialize::ExecuteBlock { job_id, .. } => job_id,
            MessageSerialize::ExecuteAppletBlock { job_id, .. } => job_id,
        }
    }
}

impl MessageDeserialize {
    pub fn session_id(&self) -> &str {
        match self {
            MessageDeserialize::BlockInputs { session_id, .. } => session_id,
            MessageDeserialize::ExecuteBlock { session_id, .. } => session_id,
            MessageDeserialize::ExecuteAppletBlock { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> &str {
        match self {
            MessageDeserialize::BlockInputs { job_id, .. } => job_id,
            MessageDeserialize::ExecuteBlock { job_id, .. } => job_id,
            MessageDeserialize::ExecuteAppletBlock { job_id, .. } => job_id,
        }
    }
}

#[async_trait]
pub trait SchedulerTxImpl {
    async fn send_inputs(&self, job_id: &JobId, data: MessageData);
    async fn run_block(&self, executor_name: &String, data: MessageData);
    async fn run_applet_block(&self, executor_name: &String, data: MessageData);
    async fn send_drop_message(&self, executor_name: &String, data: MessageData);
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
    SendExecute((String, MessageData)),
    SendApplet((String, MessageData)),
    SendDrop((String, Vec<u8>)),
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
        inputs: Option<&BlockInputs>, inputs_def: &Option<InputHandles>,
    ) {
        let data = serde_json::to_vec(&MessageSerialize::BlockInputs {
            session_id: &self.session_id,
            job_id: &job_id,
            inputs,
            stacks,
            block_path,
            inputs_def,
        })
        .unwrap();
        self.tx.send(Command::SendInputs((job_id, data))).unwrap()
    }

    pub fn send_to_executor(
        &self, executor_name: String, job_id: JobId, stacks: &Vec<BlockJobStackLevel>, dir: String,
        executor: &TaskBlockExecutor, outputs: &Option<OutputHandles>,
    ) {
        let data = serde_json::to_vec(&MessageSerialize::ExecuteBlock {
            executor_name: &executor_name,
            session_id: &self.session_id,
            job_id: &job_id,
            stacks,
            dir: &dir,
            executor,
            outputs,
        })
        .unwrap();
        self.tx
            .send(Command::SendExecute((executor_name, data)))
            .unwrap()
    }

    pub fn send_to_applet(
        &self, executor_name: String, block_name: String, job_id: JobId,
        stacks: &Vec<BlockJobStackLevel>, dir: String, executor: &AppletExecutorOptions,
        outputs: &Option<OutputHandles>,
    ) {
        let data = serde_json::to_vec(&MessageSerialize::ExecuteAppletBlock {
            session_id: &self.session_id,
            job_id: &job_id,
            executor_name: &executor_name,
            dir: &dir,
            block_name: &block_name,
            applet_executor: executor,
            stacks,
            outputs: outputs,
        })
        .unwrap();
        self.tx
            .send(Command::SendApplet((executor_name, data)))
            .unwrap()
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

impl DropSender for SchedulerTx {
    fn drop_message(&self, data: Vec<u8>) {
        // TODO: 在发送时，应该确定好 data 的格式数据。使用 utils 里面的 OutputRef struct 作为格式。
        match serde_json::from_slice::<utils::output::OutputRef>(&data) {
            Ok(obj_ref) => {
                let executor = obj_ref.executor;
                self.tx.send(Command::SendDrop((executor, data))).unwrap();
            }
            Err(e) => {
                eprintln!("Incorrect message sending to scheduler: {:?}", e);
            }
        }
    }
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
                    Ok(Command::SendApplet((executor, data))) => {
                        impl_tx.run_applet_block(&executor, data).await;
                    }
                    Ok(Command::SendExecute((executor, data))) => {
                        impl_tx.run_block(&executor, data).await;
                    }
                    Ok(Command::SendDrop((executor, data))) => {
                        impl_tx.send_drop_message(&executor, data).await;
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
