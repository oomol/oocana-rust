use std::{collections::HashMap, sync::Arc};

use flume::{Receiver, Sender};
use job::JobId;
use utils::output::OutputValue;

use manifest_meta::HandleName;

pub enum Status {
    Output {
        job_id: JobId,
        result: Arc<OutputValue>,
        handle: HandleName,
        done: bool,
    },
    OutputMap {
        job_id: JobId,
        map: HashMap<HandleName, Arc<OutputValue>>,
        done: bool,
    },
    Done {
        job_id: JobId,
        result: Option<HashMap<HandleName, Arc<OutputValue>>>,
        error: Option<String>,
    },
    Error {
        error: String,
    },
}

#[derive(Clone)]
pub struct BlockStatusTx {
    tx: Sender<Status>,
}

impl BlockStatusTx {
    pub fn output(&self, job_id: JobId, result: Arc<OutputValue>, handle: HandleName, done: bool) {
        self.tx
            .send(Status::Output {
                job_id,
                result,
                handle,
                done,
            })
            .unwrap();
    }
    pub fn output_map(
        &self,
        job_id: JobId,
        map: HashMap<HandleName, Arc<OutputValue>>,
        done: bool,
    ) {
        self.tx
            .send(Status::OutputMap { job_id, map, done })
            .unwrap();
    }
    pub fn done(
        &self,
        job_id: JobId,
        result: Option<HashMap<HandleName, Arc<OutputValue>>>,
        error: Option<String>,
    ) {
        self.tx
            .send(Status::Done {
                job_id,
                result,
                error,
            })
            .unwrap();
    }
    pub fn error(&self, error: String) {
        self.tx.send(Status::Error { error }).unwrap();
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
