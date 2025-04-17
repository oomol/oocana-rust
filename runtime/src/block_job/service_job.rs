use std::sync::Arc;

use job::{BlockInputs, BlockJobStacks, JobId};
use mainframe::scheduler::{SchedulerTx, ServiceParams};
use manifest_meta::{
    InjectionStore, InputDefPatchMap, RunningScope, ServiceBlock, ServiceExecutorOptions,
    SubflowBlock,
};

use super::block::BlockJobHandle;
use super::listener::{listen_to_worker, ListenerArgs, ServiceExecutorPayload};
use crate::{block_status::BlockStatusTx, shared::Shared};

pub struct ServiceJobHandle {
    pub job_id: JobId,
    shared: Arc<Shared>,
    spawn_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for ServiceJobHandle {
    fn drop(&mut self) {
        self.shared
            .scheduler_tx
            .unregister_subscriber(self.job_id.to_owned());
        self.shared
            .delay_abort_tx
            .send(self.spawn_handles.drain(..).collect());
    }
}

pub struct RunServiceBlockArgs {
    pub service_block: Arc<ServiceBlock>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    pub injection_store: Option<InjectionStore>,
    pub scope: RunningScope,
    pub parent_flow: Option<Arc<SubflowBlock>>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
}

pub fn run_service_block(args: RunServiceBlockArgs) -> Option<BlockJobHandle> {
    let RunServiceBlockArgs {
        service_block,
        shared,
        stacks,
        job_id,
        inputs,
        block_status,
        injection_store,
        scope,
        parent_flow,
        inputs_def_patch,
    } = args;
    let reporter = Arc::new(
        shared
            .reporter
            .block(job_id.to_owned(), None, stacks.clone()),
    );

    reporter.started(&inputs);
    let service_options = service_executor_options(&service_block);
    let worker_listener_handle = listen_to_worker(ListenerArgs {
        job_id: job_id.to_owned(),
        block_path: None,
        stacks: stacks.clone(),
        scheduler_tx: shared.scheduler_tx.clone(),
        inputs,
        outputs_def: service_block.outputs_def.clone(),
        inputs_def: service_block.inputs_def.clone(),
        block_status,
        reporter: Arc::clone(&reporter),
        executor: None,
        scope: scope.clone(),
        service: Some(ServiceExecutorPayload {
            options: service_options,
            block_name: service_block.name.clone(),
            executor_name: service_block
                .service_executor
                .as_ref()
                .as_ref()
                .unwrap()
                .name
                .clone()
                .unwrap(),
        }),
        block_dir: service_dir(&service_block),
        injection_store,
        flow: parent_flow.as_ref().map(|f| f.path_str.clone()),
        inputs_def_patch,
    });

    send_to_service(
        &service_block,
        &job_id,
        shared.scheduler_tx.clone(),
        stacks,
        &scope,
        parent_flow.as_ref().map(|f| f.path_str.clone()),
    );

    let spawn_handles: Vec<tokio::task::JoinHandle<()>> = vec![worker_listener_handle];

    Some(BlockJobHandle::new(
        job_id.to_owned(),
        ServiceJobHandle {
            job_id,
            shared,
            spawn_handles,
        },
    ))
}

// TODO: 有重复，回头合并
fn service_dir(service_block: &ServiceBlock) -> String {
    match &service_block.service_path {
        Some(path) => path.parent().unwrap().to_str().unwrap().to_owned(),
        None => ".".to_owned(),
    }
}

fn service_executor_options(service_block: &ServiceBlock) -> ServiceExecutorOptions {
    let executor = service_block.service_executor.as_ref().as_ref().unwrap();
    ServiceExecutorOptions {
        name: executor.name.clone(),
        entry: executor.entry.clone(),
        function: executor.function.clone(),
        start_at: executor.start_at.clone(),
        stop_at: executor.stop_at.clone(),
        keep_alive: executor.keep_alive,
    }
}

fn send_to_service(
    service_block: &ServiceBlock,
    job_id: &JobId,
    scheduler_tx: SchedulerTx,
    stacks: BlockJobStacks,
    scope: &RunningScope,
    flow: Option<String>,
) {
    let executor_name = service_block
        .service_executor
        .as_ref()
        .as_ref()
        .unwrap()
        .name
        .clone()
        .unwrap();

    let dir = service_dir(service_block);

    let service_executor_option = service_executor_options(service_block);

    scheduler_tx.send_to_service(ServiceParams {
        executor_name: &executor_name,
        block_name: &service_block.name,
        job_id: job_id.to_owned(),
        stacks: stacks.vec(),
        dir,
        options: &service_executor_option,
        outputs: &service_block.outputs_def,
        scope,
        flow: &flow,
    })
}
