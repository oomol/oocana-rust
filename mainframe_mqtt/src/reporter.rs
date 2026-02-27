use job::SessionId;
use std::{net::SocketAddr, time::Duration};
use tokio::sync::watch;
use utils::logger::STDOUT_TARGET;

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};

use mainframe::{
    MessageData,
    reporter::{ReporterRxImpl, ReporterTxImpl},
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
        let _ = self.shutdown_tx.send(());
    }
}

pub struct ReporterRx {
    rx: EventLoop,
    shutdown_rx: watch::Receiver<()>,
    session_id: String,
}

impl ReporterRxImpl for ReporterRx {
    fn event_loop(self) -> tokio::task::JoinHandle<()> {
        let Self {
            mut rx,
            mut shutdown_rx,
            session_id,
        } = self;

        // Run EventLoop::poll() in a dedicated task so it is NEVER cancelled.
        // rumqttc's poll() is not cancel-safe; dropping a pending poll() future
        // (e.g. inside tokio::select!) corrupts the EventLoop's internal state
        // and makes all subsequent poll() calls fail immediately.
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<Event>(256);

        let poll_task = tokio::spawn(async move {
            while let Ok(event) = rx.poll().await {
                if event_tx.send(event).await.is_err() {
                    break; // receiver dropped
                }
            }
        });

        tokio::spawn(async move {
            // Main loop: receive events from the poll task, stop on shutdown signal.
            // tokio::select! on mpsc::recv() is cancel-safe.
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                    event = event_rx.recv() => {
                        match event {
                            Some(Event::Incoming(Incoming::Publish(publish))) => {
                                let payload_str = String::from_utf8_lossy(&publish.payload);
                                if payload_str.contains(session_id.as_str()) {
                                    info!(target: STDOUT_TARGET, "{}", payload_str);
                                }
                            }
                            Some(_) => {}
                            None => break, // poll task exited
                        }
                    }
                }
            }

            // Drain events that the poll task already forwarded into the channel.
            // Give it a short window so in-flight broker messages can still arrive.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
            loop {
                match tokio::time::timeout_at(deadline, event_rx.recv()).await {
                    Ok(Some(Event::Incoming(Incoming::Publish(publish)))) => {
                        let payload_str = String::from_utf8_lossy(&publish.payload);
                        if payload_str.contains(session_id.as_str()) {
                            info!(target: STDOUT_TARGET, "{}", payload_str);
                        }
                    }
                    Ok(Some(_)) => {}
                    _ => break, // timeout or channel closed
                }
            }

            poll_task.abort();
            let _ = poll_task.await;

            info!("ReporterRx shutting down");
        })
    }
}

pub async fn connect(
    addr: &SocketAddr,
    session_id: SessionId,
    forward_to_console: bool,
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

    if forward_to_console {
        if let Err(e) = tx.subscribe("report", QoS::AtLeastOnce).await {
            error!("Failed to subscribe to 'report': {}", e);
        }
    }

    (
        ReporterTx { tx, shutdown_tx },
        ReporterRx {
            rx,
            shutdown_rx,
            session_id: session_id.to_string(),
        },
    )
}
