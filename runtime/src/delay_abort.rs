use utils::log_warn;

pub fn delay_abort() -> (DelayAbortTx, DelayAbortRx) {
    let (tx, rx) = flume::unbounded();

    (DelayAbortTx(tx), DelayAbortRx(rx))
}

pub struct DelayAbortTx(flume::Sender<Vec<tokio::task::JoinHandle<()>>>);

impl DelayAbortTx {
    pub fn send(&self, handles: Vec<tokio::task::JoinHandle<()>>) {
        self.0.send(handles).unwrap_or_else(log_warn)
    }
}

pub struct DelayAbortRx(flume::Receiver<Vec<tokio::task::JoinHandle<()>>>);

const DELAY: u64 = 100;

impl DelayAbortRx {
    pub fn run(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Ok(handles) = self.0.recv_async().await {
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(DELAY)).await;
                    for handle in handles {
                        handle.abort();
                    }
                });
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(DELAY + 500)).await;
        })
    }
}
