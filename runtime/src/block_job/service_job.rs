use std::sync::{Arc, RwLock};

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use mainframe::scheduler::{SchedulerTx, ServiceParams};
use manifest_meta::{
    InjectionStore, InputDefPatchMap, ServiceBlock, ServiceExecutorOptions, SubflowBlock,
};

use super::job_handle::BlockJobHandle;
use super::listener::{listen_to_worker, ListenerParameters, ServiceExecutorPayload};
use crate::{block_status::BlockStatusTx, shared::Shared};

/// Get the executor name from a ServiceBlock.
/// Panics with a descriptive message if the service_executor or its name is not set.
fn get_executor_name(service_block: &ServiceBlock) -> String {
    service_block
        .service_executor
        .as_ref()
        .as_ref()
        .expect("service_executor must be set for service blocks")
        .name
        .clone()
        .expect("service_executor.name must be set")
}

/// Get the ServiceExecutorOptions from a ServiceBlock.
/// Panics with a descriptive message if the service_executor is not set.
fn get_service_executor(service_block: &ServiceBlock) -> &ServiceExecutorOptions {
    service_block
        .service_executor
        .as_ref()
        .as_ref()
        .expect("service_executor must be set for service blocks")
}

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

pub struct ServiceJobParameters {
    pub service_block: Arc<ServiceBlock>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    pub injection_store: Option<InjectionStore>,
    pub scope: RuntimeScope,
    pub parent_flow: Option<Arc<RwLock<SubflowBlock>>>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
}

pub fn execute_service_job(params: ServiceJobParameters) -> Option<BlockJobHandle> {
    let ServiceJobParameters {
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
    } = params;
    let reporter = Arc::new(
        shared
            .reporter
            .block(job_id.to_owned(), None, stacks.clone()),
    );

    reporter.started(&inputs);
    let service_options = service_executor_options(&service_block);
    let worker_listener_handle = listen_to_worker(ListenerParameters {
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
            executor_name: get_executor_name(&service_block),
        }),
        block_dir: service_dir(&service_block),
        injection_store,
        flow_path: parent_flow.as_ref().map(|f| f.read().unwrap().path_str.clone()),
        inputs_def_patch,
    });

    send_to_service(
        &service_block,
        &job_id,
        shared.scheduler_tx.clone(),
        stacks,
        &scope,
        parent_flow.as_ref().map(|f| f.read().unwrap().path_str.clone()),
    );

    let spawn_handles: Vec<tokio::task::JoinHandle<()>> = vec![worker_listener_handle];

    Some(BlockJobHandle::new(ServiceJobHandle {
        job_id,
        shared,
        spawn_handles,
    }))
}

// TODO: 有重复，回头合并
fn service_dir(service_block: &ServiceBlock) -> String {
    match &service_block.service_path {
        Some(path) => path.parent().unwrap().to_str().unwrap().to_owned(),
        None => ".".to_owned(),
    }
}

fn service_executor_options(service_block: &ServiceBlock) -> ServiceExecutorOptions {
    let executor = get_service_executor(service_block);
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
    scope: &RuntimeScope,
    flow: Option<String>,
) {
    let executor_name = get_executor_name(service_block);

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
        flow_path: &flow,
    })
}
