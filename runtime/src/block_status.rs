use std::sync::Arc;

use flume::{Receiver, Sender};
use job::JobId;
use utils::output::OutputValue;

use manifest_meta::HandleName;

pub enum Status {
    Result {
        job_id: JobId,
        result: Arc<OutputValue>,
        handle: HandleName,
        done: bool,
    },
    Done {
        job_id: JobId,
        error: Option<String>,
    },
}

#[derive(Clone)]
pub struct BlockStatusTx {
    tx: Sender<Status>,
}

impl BlockStatusTx {
    pub fn result(&self, job_id: JobId, result: Arc<OutputValue>, handle: HandleName, done: bool) {
        self.tx
            .send(Status::Result {
                job_id,
                result,
                handle,
                done,
            })
            .unwrap();
    }
    pub fn done(&self, job_id: JobId, error: Option<String>) {
        self.tx.send(Status::Done { job_id, error }).unwrap();
    }
}

pub struct BlockStatusRx {
    rx: Receiver<Status>,
}

impl BlockStatusRx {
    pub async fn recv(&self) -> Option<Status> {
        self.rx.recv_async().await.ok()
    }
}

pub fn create() -> (BlockStatusTx, BlockStatusRx) {
    let (tx, rx) = flume::unbounded();
    (BlockStatusTx { tx }, BlockStatusRx { rx })
}
