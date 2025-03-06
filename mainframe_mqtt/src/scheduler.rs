use std::{net::SocketAddr, time::Duration};

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};

use job::{JobId, SessionId};
use mainframe::{
    scheduler::{SchedulerRxImpl, SchedulerTxImpl},
    MessageData,
};

pub struct SchedulerTx {
    session_id: SessionId,
    tx: AsyncClient,
}

#[async_trait]
impl SchedulerTxImpl for SchedulerTx {
    async fn send_inputs(&self, job_id: &JobId, data: MessageData) {
        let topic = format!("inputs/{}/{}", &self.session_id, job_id);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn run_block(&self, executor: &String, data: MessageData) {
        let topic = format!("executor/{}/run_block", executor);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn run_applet_block(&self, executor: &String, data: MessageData) {
        let topic = format!("executor/{}/run_applet_block", executor);

        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn send_drop_message(&self, executor: &String, data: MessageData) {
        let topic = format!("executor/{}/drop", executor);
        self.tx
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap()
    }

    async fn disconnect(&self) {
        self.tx.disconnect().await.unwrap();
    }
}

pub struct SchedulerRx {
    rx: EventLoop,
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
                    eprintln!("Cannot connect Oocana Scheduler to broker. error: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

pub async fn connect(addr: &SocketAddr, session_id: SessionId) -> (SchedulerTx, SchedulerRx) {
    let mut options = MqttOptions::new(
        &format!("oocana-scheduler-{}", &session_id),
        addr.ip().to_string(),
        addr.port(),
    );
    options.set_max_packet_size(268435456, 268435456);
    options.set_keep_alive(Duration::from_secs(60));

    let (tx, rx) = AsyncClient::new(options, 50);

    let channel = format!("session/{}", &session_id);

    tx.subscribe(&channel, QoS::AtLeastOnce).await.unwrap();

    (SchedulerTx { tx, session_id }, SchedulerRx { rx })
}
