use std::{net::SocketAddr, time::Duration};

use async_trait::async_trait;
use job::{JobId, SessionId};
use mainframe::{
    scheduler::{SchedulerRxImpl, SchedulerTxImpl},
    MessageData,
};
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};
use tokio::sync::watch;
use tracing::{error, info};

pub struct SchedulerTx {
    session_id: SessionId,
    tx: AsyncClient,
    shutdown_tx: watch::Sender<bool>,
}

#[async_trait]
impl SchedulerTxImpl for SchedulerTx {
    async fn send_block_event(&self, session_id: &SessionId, data: MessageData) {
        let topic = format!("session/{}", session_id);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn send_inputs(&self, job_id: &JobId, data: MessageData) {
        let topic = format!("inputs/{}/{}", &self.session_id, job_id);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn run_block(&self, executor: &str, data: MessageData) {
        let topic = format!("executor/{}/run_block", executor);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn respond_block_request(
        &self,
        session_id: &SessionId,
        request_id: &str,
        data: MessageData,
    ) {
        let topic = format!("session/{}/request/{}/response", session_id, request_id);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn run_service_block(&self, executor: &str, data: MessageData) {
        let topic = format!("executor/{}/run_service_block", executor);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn disconnect(&self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.tx.disconnect().await;
    }
}

pub struct SchedulerRx {
    rx: EventLoop,
    shutdown_rx: watch::Receiver<bool>,
}

#[async_trait]
impl SchedulerRxImpl for SchedulerRx {
    async fn recv(&mut self) -> MessageData {
        loop {
            match self.rx.poll().await {
                Ok(notification) => {
                    if let Event::Incoming(Incoming::Publish(packet)) = notification {
                        return packet.payload.into();
                    }
                }
                Err(e) => {
                    let is_shutdown = self.shutdown_rx.borrow();
                    if *is_shutdown {
                        info!("scheduler is shutting down");
                        break;
                    }
                    // TODO: need distinguish between normal error and unrecoverable error
                    error!("exit because scheduler error: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        MessageData::default()
    }
}

pub async fn connect(addr: &SocketAddr, session_id: SessionId) -> (SchedulerTx, SchedulerRx) {
    let mut options = MqttOptions::new(
        format!("oocana-scheduler-{}", &session_id),
        addr.ip().to_string(),
        addr.port(),
    );
    options.set_max_packet_size(268435456, 268435456);
    options.set_keep_alive(Duration::from_secs(60));

    let (tx, rx) = AsyncClient::new(options, 50);

    let channel = format!("session/{}", &session_id);

    tx.subscribe(&channel, QoS::AtLeastOnce).await.unwrap();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    (
        SchedulerTx {
            tx,
            session_id,
            shutdown_tx,
        },
        SchedulerRx { rx, shutdown_rx },
    )
}
