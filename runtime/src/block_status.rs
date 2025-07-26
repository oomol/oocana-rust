use std::{collections::HashMap, sync::Arc};

use flume::{Receiver, Sender};
use job::JobId;
use mainframe::scheduler::{BlockRequest, OutputOptions};
use utils::output::OutputValue;

use manifest_meta::HandleName;

pub enum Status {
    Output {
        job_id: JobId,
        result: Arc<OutputValue>,
        handle: HandleName,
        options: Option<OutputOptions>,
    },
    Outputs {
        job_id: JobId,
        outputs: HashMap<HandleName, Arc<OutputValue>>,
    },
    Progress {
        job_id: JobId,
        progress: f32,
    },
    Request(BlockRequest),
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
    pub fn output(
        &self,
        job_id: JobId,
        result: Arc<OutputValue>,
        handle: HandleName,
        options: Option<OutputOptions>,
    ) {
        self.tx
            .send(Status::Output {
                job_id,
                result,
                handle,
                options,
            })
            .unwrap();
    }
    pub fn outputs(&self, job_id: JobId, outputs: HashMap<HandleName, Arc<OutputValue>>) {
        self.tx.send(Status::Outputs { job_id, outputs }).unwrap();
    }
    pub fn progress(&self, job_id: JobId, progress: f32) {
        self.tx.send(Status::Progress { job_id, progress }).unwrap();
    }
    pub fn run_request(&self, request: BlockRequest) {
        self.tx.send(Status::Request(request)).unwrap();
    }
    pub fn finish(
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

    // error function don't have job_id, it is a global error not related to a specific job. Currently code architecture only doesn't support handle global error, so we use this function to send global error.
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
