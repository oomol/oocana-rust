use job::SessionId;
use std::{net::SocketAddr, time::Duration};
use tokio::sync::watch;
use utils::logger::STDOUT_TARGET;

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};

use mainframe::{
    reporter::{ReporterRxImpl, ReporterTxImpl},
    MessageData,
};
use tracing::{error, info};

pub struct ReporterTx {
    tx: AsyncClient,
    shutdown_tx: watch::Sender<()>,
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
                        match result {
                            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                                let payload = publish.payload;
                                let payload_str = String::from_utf8_lossy(&payload);
                                info!(target:STDOUT_TARGET, "{}", payload_str);
                            }
                            Ok(_) => {}
                            Err(e) => {
                                error!("Error in reporter event loop: {}", e);
                            }
                        }
                    }
                }
            }
        })
    }
}

pub async fn connect(
    addr: &SocketAddr,
    session_id: SessionId,
    send_to_console: bool,
) -> (ReporterTx, ReporterRx) {
    let mut options = MqttOptions::new(
        format!("oocana-reporter-{}", session_id),
        addr.ip().to_string(),
        addr.port(),
    );
    options.set_max_packet_size(268435456, 268435456);
    options.set_keep_alive(Duration::from_secs(60));

    let (shutdown_tx, shutdown_rx) = watch::channel(());
    let (tx, rx) = AsyncClient::new(options, 50);

    if send_to_console {
        tx.subscribe("report", QoS::AtLeastOnce).await.unwrap();
    }

    (
        ReporterTx { tx, shutdown_tx },
        ReporterRx { rx, shutdown_rx },
    )
}
