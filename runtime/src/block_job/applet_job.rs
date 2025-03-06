use std::sync::Arc;

use job::{BlockInputs, BlockJobStacks, JobId};
use mainframe::scheduler::SchedulerTx;
use manifest_meta::{AppletBlock, AppletExecutorOptions};

use crate::{block_status::BlockStatusTx, shared::Shared};

use super::{task_job::listen_to_worker, BlockJobHandle};

pub struct AppletJobHandle {
    pub job_id: JobId,
    shared: Arc<Shared>,
    spawn_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for AppletJobHandle {
    fn drop(&mut self) {
        self.shared
            .scheduler_tx
            .unregister_subscriber(self.job_id.to_owned());
        self.shared
            .delay_abort_tx
            .send(self.spawn_handles.drain(..).collect());
    }
}

pub fn run_applet_block(
    applet_block: Arc<AppletBlock>, shared: Arc<Shared>, stacks: BlockJobStacks, job_id: JobId,
    inputs: Option<BlockInputs>, block_status: BlockStatusTx,
) -> Option<BlockJobHandle> {
    let reporter = Arc::new(
        shared
            .reporter
            .block(job_id.to_owned(), None, stacks.clone()),
    );

    reporter.started(&inputs);

    let worker_listener_handle = listen_to_worker(
        job_id.to_owned(),
        None,
        stacks.clone(),
        shared.scheduler_tx.clone(),
        inputs,
        applet_block.outputs_def.clone(),
        applet_block.inputs_def.clone(),
        block_status,
        Arc::clone(&reporter),
    );

    send_to_applet(&applet_block, &job_id, shared.scheduler_tx.clone(), stacks);

    let mut spawn_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    spawn_handles.push(worker_listener_handle);

    Some(BlockJobHandle::new(
        job_id.to_owned(),
        AppletJobHandle {
            job_id,
            shared,
            spawn_handles,
        },
    ))
}

fn send_to_applet(
    applet_block: &AppletBlock, job_id: &JobId, scheduler_tx: SchedulerTx, stacks: BlockJobStacks,
) {
    let executor_name = applet_block
        .applet_executor
        .as_ref()
        .as_ref()
        .unwrap()
        .name
        .clone()
        .unwrap();

    let dir = match &applet_block.applet_path {
        Some(path) => path.parent().unwrap().to_str().unwrap().to_owned(),
        None => ".".to_owned(),
    };

    let applet_executor_option: AppletExecutorOptions = {
        let executor = applet_block.applet_executor.as_ref().as_ref().unwrap();
        AppletExecutorOptions {
            name: executor.name.clone(),
            entry: executor.entry.clone(),
            function: executor.function.clone(),
            start_at: executor.start_at.clone(),
            stop_at: executor.stop_at.clone(),
            keep_alive: executor.keep_alive.clone(),
        }
    };

    scheduler_tx.send_to_applet(
        executor_name,
        applet_block.name.to_owned(),
        job_id.to_owned(),
        stacks.vec(),
        dir,
        &applet_executor_option,
        &applet_block.outputs_def,
    )
}
