use std::{net::SocketAddr, time::Duration};

use async_trait::async_trait;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};

use mainframe::{
    reporter::{ReporterRxImpl, ReporterTxImpl},
    MessageData,
};
use uuid::Uuid;

pub struct ReporterTx {
    tx: AsyncClient,
}

#[async_trait]
impl ReporterTxImpl for ReporterTx {
    async fn send(&self, data: MessageData) {
        self.tx
            .publish("report", QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn disconnect(&self) {
        self.tx.disconnect().await.unwrap();
    }
}

pub struct ReporterRx {
    rx: EventLoop,
}

impl ReporterRxImpl for ReporterRx {
    fn event_loop(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.rx.poll().await {
                    eprintln!("Cannot connect Oocana Reporter to broker. error: {:?}", e);
                    std::process::exit(1);
                }
            }
        })
    }
}

pub async fn connect(addr: &SocketAddr) -> (ReporterTx, ReporterRx) {
    let mut options = MqttOptions::new(
        format!("oocana-reporter-{}", Uuid::new_v4().to_string()),
        addr.ip().to_string(),
        addr.port(),
    );
    options.set_max_packet_size(268435456, 268435456);
    options.set_keep_alive(Duration::from_secs(60));

    let (tx, rx) = AsyncClient::new(options, 50);

    (ReporterTx { tx }, ReporterRx { rx })
}
