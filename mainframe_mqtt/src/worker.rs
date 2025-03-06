use std::{net::SocketAddr, time::Duration};

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};

use job::{JobId, SessionId};
use mainframe::{
    worker::{WorkerRxImpl, WorkerTxImpl},
    MessageData,
};

pub struct WorkerTx {
    topic: String,
    tx: AsyncClient,
}

#[async_trait]
impl WorkerTxImpl for WorkerTx {
    async fn send(&self, data: MessageData) {
        self.tx
            .publish(&self.topic, QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }
}

pub struct WorkerRx {
    rx: EventLoop,
}

#[async_trait]
impl WorkerRxImpl for WorkerRx {
    async fn recv(&mut self) -> MessageData {
        loop {
            match self.rx.poll().await {
                Ok(notification) => {
                    if let Event::Incoming(Incoming::Publish(packet)) = notification {
                        return packet.payload.into();
                    }
                }
                Err(e) => {
                    eprintln!("Cannot connect Oocana Worker to broker. error: {:?}", e);
                }
            }
        }
    }
}

pub async fn connect(
    addr: SocketAddr, session_id: SessionId, job_id: JobId,
) -> (WorkerTx, WorkerRx) {
    let mut options = MqttOptions::new(
        &format!("oocana-worker-{}", &job_id),
        addr.ip().to_string(),
        addr.port(),
    );
    options.set_max_packet_size(268435456, 268435456);
    options.set_keep_alive(Duration::from_secs(60));

    let (tx, rx) = AsyncClient::new(options, 50);

    tx.subscribe(
        format!("inputs/{}/{}", &session_id, &job_id),
        QoS::AtLeastOnce,
    )
    .await
    .unwrap();

    (
        WorkerTx {
            tx,
            topic: format!("session/{}", &session_id),
        },
        WorkerRx { rx },
    )
}
