use std::sync::Arc;

use flume::{Receiver, Sender};
use job::JobId;
use manifest_meta::{HandleName, JsonValue};

pub enum Status {
    Result {
        job_id: JobId,
        result: Arc<JsonValue>,
        handle: HandleName,
        done: bool,
    },
    Done {
        job_id: JobId,
    },
}

#[derive(Clone)]
pub struct BlockStatusTx {
    tx: Sender<Status>,
}

impl BlockStatusTx {
    pub fn result(&self, job_id: JobId, result: Arc<JsonValue>, handle: HandleName, done: bool) {
        self.tx
            .send(Status::Result {
                job_id,
                result,
                handle,
                done,
            })
            .unwrap();
    }
    pub fn done(&self, job_id: JobId) {
        self.tx.send(Status::Done { job_id }).unwrap();
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
