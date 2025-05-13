use std::{net::SocketAddr, time::Duration};
use tokio::sync::watch;

use async_trait::async_trait;
use rumqttc::{AsyncClient, EventLoop, MqttOptions, QoS};

use mainframe::{
    reporter::{ReporterRxImpl, ReporterTxImpl},
    MessageData,
};
use tracing::{error, info};
use uuid::Uuid;

pub struct ReporterTx {
    tx: AsyncClient,
    shutdown_tx: watch::Sender<()>,
}

#[async_trait]
impl ReporterTxImpl for ReporterTx {
    async fn send(&self, suffix: String, data: MessageData) {
        self.tx
            .publish(format!("report/{}", suffix), QoS::AtLeastOnce, false, data)
            .await
            .unwrap();
    }

    async fn disconnect(&self) {
        let _ = self.tx.disconnect().await;
        let _ = self.shutdown_tx.send(());
    }
}

pub struct ReporterRx {
    rx: EventLoop,
    shutdown_rx: watch::Receiver<()>,
}

impl ReporterRxImpl for ReporterRx {
    fn event_loop(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = self.shutdown_rx.changed() => {
                        info!("ReporterRx shutting down");
                        break;
                    }
                    result = self.rx.poll() => {
                        if let Err(e) = result {
                            error!("Cannot connect Oocana Reporter to broker. error: {:?}", e);
                        }
                    }
                }
            }
        })
    }
}

pub async fn connect(addr: &SocketAddr) -> (ReporterTx, ReporterRx) {
    let mut options = MqttOptions::new(
        format!("oocana-reporter-{}", Uuid::new_v4()),
        addr.ip().to_string(),
        addr.port(),
    );
    options.set_max_packet_size(268435456, 268435456);
    options.set_keep_alive(Duration::from_secs(60));
    let (shutdown_tx, shutdown_rx) = watch::channel(());
    let (tx, rx) = AsyncClient::new(options, 50);

    (
        ReporterTx { tx, shutdown_tx },
        ReporterRx { rx, shutdown_rx },
    )
}
