use std::{
    collections::{HashMap, HashSet},
    default,
    sync::{Arc, RwLock},
};

use crate::{
    block_job::{self, execute_task_job, BlockJobHandle, TaskJobParameters},
    block_status::{self, BlockStatusTx},
    flow_job::{
        block_request::{
            parse_node_downstream, parse_query_block_request, parse_run_block_request,
            RunBlockSuccessResponse,
        },
        cache::save_flow_cache,
        find_upstream_nodes, parse_oauth_request,
    },
    run::{run_job, CommonJobParameters, JobParams},
    shared::Shared,
};
use mainframe::{
    reporter::{ErrorDetail, FlowReporterTx},
    scheduler::{
        self, BlockRequest, BlockResponseParams, OutputOptions, QueryBlockRequest, ToFlowOutput,
        ToNodeInput,
    },
};
use tracing::warn;
use utils::output::OutputValue;

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use manifest_meta::{
    Block, BlockResolver, BlockScope, HandleTo, InputHandle, Node, NodeId, Slot, SubflowBlock,
};

use super::node_input_values;
use node_input_values::NodeInputValues;

pub struct FlowJobHandle {
    spawn_handle: tokio::task::JoinHandle<()>,
}

impl Drop for FlowJobHandle {
    fn drop(&mut self) {
        self.spawn_handle.abort();
    }
}

struct BlockInFlowJobHandle {
    node_id: NodeId,
    _job: BlockJobHandle,
}

struct FlowShared {
    job_id: JobId,
    flow_block: Arc<RwLock<SubflowBlock>>,
    shared: Arc<Shared>,
    stacks: BlockJobStacks,
    parent_scope: RuntimeScope,
    scope: RuntimeScope,
    slot_blocks: HashMap<NodeId, Slot>,
    path_finder: manifest_reader::path_finder::BlockPathFinder,
    vault_client: Arc<Option<vault::VaultClient>>,
}

struct RunFlowContext {
    node_input_values: NodeInputValues,
    parent_block_status: BlockStatusTx,
    jobs: HashMap<JobId, BlockInFlowJobHandle>,
    block_status: BlockStatusTx,
    node_queue_pool: HashMap<NodeId, NodeQueue>,
}

#[derive(Default)]
struct NodeQueue {
    pub jobs: HashSet<JobId>,
    pub pending: HashSet<JobId>, // JobId 本身不会使用，只是用来判断 pending 的数量
}

pub struct FlowJobParameters {
    pub flow_block: Arc<RwLock<SubflowBlock>>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub flow_job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub node_value_store: NodeInputValues,
    pub parent_block_status: BlockStatusTx,
    pub nodes: Option<HashSet<NodeId>>,
    pub parent_scope: RuntimeScope,
    pub scope: RuntimeScope,
    pub slot_blocks: HashMap<NodeId, Slot>,
    pub path_finder: manifest_reader::path_finder::BlockPathFinder,
    pub vault_client: Arc<Option<vault::VaultClient>>,
}

pub fn execute_flow_job(mut params: FlowJobParameters) -> Option<BlockJobHandle> {
    let FlowJobParameters {
        flow_block,
        shared,
        stacks,
        flow_job_id,
        inputs,
        parent_block_status,
        node_value_store,
        ref mut nodes,
        slot_blocks,
        scope,
        parent_scope,
        path_finder,
        vault_client,
    } = params;

    // Acquire read lock to get necessary data
    let (flow_path_str, flow_path, absence_node_inputs) = {
        let flow_guard = flow_block.read().unwrap();
        let absence_node_inputs = flow_guard
            .query_nodes_inputs()
            .into_iter()
            .filter_map(|(node_id, handles)| {
                if flow_guard
                    .nodes
                    .get(&node_id)
                    .is_none_or(|n| node_value_store.is_node_fulfill(n))
                {
                    None
                } else {
                    Some((
                        node_id,
                        handles
                            .iter()
                            .filter(|h| h.value.is_none())
                            .cloned()
                            .collect::<Vec<InputHandle>>(),
                    ))
                }
            })
            .collect::<HashMap<NodeId, Vec<InputHandle>>>();
        (flow_guard.path_str.clone(), flow_guard.path.clone(), absence_node_inputs)
    };

    let reporter = Arc::new(shared.reporter.flow(
        flow_job_id.to_owned(),
        Some(flow_path_str.clone()),
        stacks.clone(),
    ));
    reporter.started(&inputs);

    if !absence_node_inputs.is_empty() {
        let node_and_handles = absence_node_inputs
            .iter()
            .map(|(node_id, handles)| {
                let handles_name = handles
                    .iter()
                    .map(|input| input.handle.to_string())
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("node({}) handles: [{}]", node_id, handles_name)
            })
            .collect::<Vec<String>>()
            .join(", ");
        // these message is tested in tests/test_run.rs
        warn!(
            "these node won't run because some inputs are not provided: {}",
            node_and_handles,
        );
    }

    let (block_status_tx, block_status_rx) = block_status::create();

    // Build limit_nodes: if Some, run only those nodes and all their upstream dependencies; None means run all nodes.
    let mut limit_nodes = nodes.clone();
    let mut run_flow_ctx = RunFlowContext {
        node_input_values: node_value_store,
        parent_block_status,
        jobs: HashMap::new(),
        block_status: block_status_tx,
        node_queue_pool: HashMap::new(),
    };

    let flow_shared = FlowShared {
        flow_block,
        job_id: flow_job_id.to_owned(),
        shared,
        stacks,
        scope,
        slot_blocks,
        parent_scope,
        path_finder: path_finder.subflow(flow_path),
        vault_client: vault_client.clone(),
    };

    if let Some(ref origin_nodes) = nodes {
        let (runnable_nodes, pending_nodes, upstream_nodes) = {
            let flow_guard = flow_shared.flow_block.read().unwrap();
            find_upstream_nodes(
                origin_nodes,
                &flow_guard,
                &mut run_flow_ctx.node_input_values,
            )
        };

        reporter.will_run_nodes(
            &runnable_nodes,
            &pending_nodes,
            &origin_nodes
                .iter()
                .map(|node| node.to_string())
                .collect::<Vec<String>>(),
        );

        for node in runnable_nodes {
            let node_opt = {
                let flow_guard = flow_shared.flow_block.read().unwrap();
                flow_guard.nodes.get(&NodeId::from(node.clone())).cloned()
            };
            if let Some(node) = node_opt {
                run_node(&node, &flow_shared, &mut run_flow_ctx);
            }
        }

        // add upstream nodes to filtered_nodes
        for node in upstream_nodes {
            limit_nodes
                .as_mut()
                .map(|nodes| nodes.insert(NodeId::from(node)));
        }
    } else {
        let mut runnable_nodes: Vec<String> = Vec::new();
        let mut pending_nodes: Vec<String> = Vec::new();
        {
            let flow_guard = flow_shared.flow_block.read().unwrap();
            for node in flow_guard.nodes.values() {
                if run_flow_ctx.node_input_values.is_node_fulfill(node) {
                    runnable_nodes.push(node.node_id().to_string());
                } else {
                    pending_nodes.push(node.node_id().to_string());
                }
            }
        }

        // 直接把可直接运行之外的节点，都当做中间节点（可以考虑把没有 output 连线的节点当做终点）
        // 目前 UI 只会区分可直接运行的节点，和其他节点（mid 和 end）
        reporter.will_run_nodes(&runnable_nodes, &pending_nodes, &Vec::new());

        for node in runnable_nodes {
            let node_opt = {
                let flow_guard = flow_shared.flow_block.read().unwrap();
                flow_guard.nodes.get(&NodeId::from(node.clone())).cloned()
            };
            if let Some(node) = node_opt {
                run_node(&node, &flow_shared, &mut run_flow_ctx);
            }
        }
    }

    if let Some(inputs) = inputs {
        for (handle, value) in inputs {
            let handle_tos_opt = {
                let flow_guard = flow_shared.flow_block.read().unwrap();
                flow_guard.flow_inputs_tos.get(&handle).cloned()
            };
            if let Some(handle_tos) = handle_tos_opt {
                produce_new_value(
                    &value,
                    &handle_tos,
                    &flow_shared,
                    &mut run_flow_ctx,
                    true,
                    &limit_nodes,
                    &reporter,
                    &None,
                );
            }
        }
    }

    if is_finish(&run_flow_ctx) {
        flow_success(&flow_shared, &run_flow_ctx, &reporter);
        return None;
    }

    struct EstimationNodeProgress {
        progress: f32,
        weight: f32,
    }

    let mut estimation_node_progress_store = HashMap::new();
    {
        let flow_guard = flow_shared.flow_block.read().unwrap();
        for (node_id, node) in flow_guard.nodes.iter() {
            estimation_node_progress_store.insert(
                node_id.clone(),
                EstimationNodeProgress {
                    progress: 0.0,
                    weight: node.progress_weight(),
                },
            );
        }
    }

    let scheduler_tx = flow_shared.shared.scheduler_tx.clone();
    let mut block_resolver = BlockResolver::new();
    let mut flow_path_finder = flow_shared.path_finder.clone();
    let vault_client = flow_shared.vault_client.clone();
    let spawn_handle = tokio::spawn(async move {
        // Initialize progress tracking variables inside the async task
        let mut total_weight = 0.0;
        {
            let flow_guard = flow_shared.flow_block.read().unwrap();
            for node in flow_guard.nodes.values() {
                total_weight += node.progress_weight();
            }
        }
        let mut estimation_progress_sum = 0.0; // 0.0 ~ 100.0

        // Helper function to update node progress
        fn update_node_progress_fn(
            progress: f32,
            estimation_node_progress: &mut EstimationNodeProgress,
            force_update: bool,
            estimation_progress_sum: &mut f32,
            total_weight: f32,
        ) -> Option<f32> {
            if (estimation_node_progress.weight == 0.0
                || estimation_node_progress.progress >= 100.0
                || (progress - estimation_node_progress.progress).abs() < f32::EPSILON)
                && !force_update
            {
                return None;
            }

            let new_progress =
                f32::max(estimation_node_progress.progress, progress).clamp(0.0, 100.0);

            let old_weight_progress =
                estimation_node_progress.progress * estimation_node_progress.weight;

            estimation_node_progress.progress = new_progress;
            let new_weight_progress = new_progress * estimation_node_progress.weight;

            *estimation_progress_sum += new_weight_progress - old_weight_progress;

            let estimation_flow_progress = if total_weight > 0.0 {
                *estimation_progress_sum / total_weight
            } else {
                return None;
            };
            // we calculate the estimation flow progress by node finish, but node can run multiple times, leave it to 95% at most.
            Some(estimation_flow_progress.clamp(0.0, 95.0))
        }

        while let Some(status) = block_status_rx.recv().await {
            match status {
                block_status::Status::Output {
                    job_id,
                    result,
                    handle,
                    options,
                } => {
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        let node_opt = {
                            let flow_guard = flow_shared.flow_block.read().unwrap();
                            flow_guard.nodes.get(&job.node_id).cloned()
                        };
                        if let Some(node) = node_opt {
                            if let Some(tos) = node.to() {
                                if let Some(handle_tos) = tos.get(&handle) {
                                    produce_new_value(
                                        &result,
                                        handle_tos,
                                        &flow_shared,
                                        &mut run_flow_ctx,
                                        true,
                                        &limit_nodes,
                                        &reporter,
                                        &options,
                                    );
                                }
                            }
                        }
                    }
                }
                block_status::Status::Progress { job_id, progress } => {
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        let contains_key = {
                            let flow_guard = flow_shared.flow_block.read().unwrap();
                            flow_guard.nodes.contains_key(&job.node_id)
                        };
                        if contains_key {
                            let node_weight_progress =
                                estimation_node_progress_store.get_mut(&job.node_id);

                            if let Some(node_weight_progress) = node_weight_progress {
                                if let Some(flow_progress) = update_node_progress_fn(
                                    progress,
                                    node_weight_progress,
                                    false,
                                    &mut estimation_progress_sum,
                                    total_weight,
                                ) {
                                    run_flow_ctx
                                        .parent_block_status
                                        .progress(flow_shared.job_id.to_owned(), flow_progress);
                                    reporter.progress(flow_progress);
                                }
                            }
                        }
                    }
                }
                block_status::Status::Outputs {
                    job_id,
                    outputs: map,
                } => {
                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        let node_opt = {
                            let flow_guard = flow_shared.flow_block.read().unwrap();
                            flow_guard.nodes.get(&job.node_id).cloned()
                        };
                        if let Some(node) = node_opt {
                            if let Some(tos) = node.to() {
                                for (handle, value) in map.iter() {
                                    if let Some(handle_tos) = tos.get(handle) {
                                        produce_new_value(
                                            value,
                                            handle_tos,
                                            &flow_shared,
                                            &mut run_flow_ctx,
                                            true,
                                            &limit_nodes,
                                            &reporter,
                                            &None,
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                block_status::Status::Request(request) => match request {
                    BlockRequest::RunBlock(request) => {
                        let res = parse_run_block_request(
                            &request,
                            &mut block_resolver,
                            &mut flow_path_finder,
                            flow_shared.shared.clone(),
                            flow_shared.scope.clone(),
                        );
                        match res {
                            Ok(response) => match response {
                                RunBlockSuccessResponse::Task {
                                    task_block,
                                    inputs,
                                    inputs_def,
                                    outputs_def,
                                    scope,
                                    request_stack,
                                    job_id,
                                    node_id,
                                } => {
                                    let block_dir = block_job::block_dir(
                                        &task_block,
                                        Some(&flow_shared.flow_block),
                                        Some(&scope),
                                    );
                                    let (flow_path_str, injection_store) = {
                                        let flow_guard = flow_shared.flow_block.read().unwrap();
                                        (flow_guard.path_str.clone(), flow_guard.injection_store.clone())
                                    };
                                    if let Some(handle) = execute_task_job(TaskJobParameters {
                                        executor: task_block.executor.clone(),
                                        block_path: task_block.path_str(),
                                        inputs_def,
                                        outputs_def,
                                        shared: flow_shared.shared.clone(),
                                        stacks: request_stack.stack(
                                            flow_shared.job_id.to_owned(),
                                            flow_path_str.clone(),
                                            node_id.clone(),
                                        ),
                                        injection_store,
                                        flow_path: Some(flow_path_str),
                                        dir: block_dir,
                                        job_id: job_id.clone(),
                                        inputs: Some(inputs),
                                        block_status: run_flow_ctx.block_status.clone(),
                                        scope,
                                        timeout: None,
                                        inputs_def_patch: None,
                                    }) {
                                        run_flow_ctx.jobs.insert(
                                            job_id.to_owned(),
                                            BlockInFlowJobHandle {
                                                node_id,
                                                _job: handle,
                                            },
                                        );
                                    }
                                }
                                RunBlockSuccessResponse::Flow {
                                    flow_block,
                                    inputs,
                                    scope,
                                    request_stack,
                                    job_id,
                                    node_id,
                                } => {
                                    let flow_path_str = {
                                        let flow_guard = flow_shared.flow_block.read().unwrap();
                                        flow_guard.path_str.clone()
                                    };
                                    if let Some(handle) = execute_flow_job(FlowJobParameters {
                                        flow_block,
                                        shared: flow_shared.shared.clone(),
                                        stacks: request_stack.stack(
                                            flow_shared.job_id.to_owned(),
                                            flow_path_str,
                                            node_id.clone(),
                                        ),
                                        flow_job_id: job_id.clone(),
                                        inputs: Some(inputs),
                                        node_value_store: NodeInputValues::new(false),
                                        parent_block_status: run_flow_ctx.block_status.clone(),
                                        nodes: None,
                                        parent_scope: flow_shared.scope.clone(),
                                        scope,
                                        slot_blocks: default::Default::default(),
                                        path_finder: flow_shared.path_finder.clone(),
                                        vault_client: flow_shared.vault_client.clone(),
                                    }) {
                                        run_flow_ctx.jobs.insert(
                                            job_id.to_owned(),
                                            BlockInFlowJobHandle {
                                                node_id,
                                                _job: handle,
                                            },
                                        );
                                    }
                                }
                            },
                            Err(err) => {
                                let msg =
                                    format!("Run block failed: {}. Block: {}", err, request.block);
                                tracing::warn!("{}", msg);
                                scheduler_tx.respond_block_request(
                                    &flow_shared.shared.session_id,
                                    scheduler::BlockResponseParams {
                                        session_id: flow_shared.shared.session_id.clone(),
                                        job_id: request.job_id.clone(),
                                        error: Some(msg),
                                        result: None,
                                        request_id: request.request_id,
                                    },
                                );
                            }
                        }
                    }
                    BlockRequest::QueryBlock(request) => {
                        let res = parse_query_block_request(
                            &request,
                            &mut block_resolver,
                            &mut flow_path_finder,
                        );
                        let QueryBlockRequest {
                            session_id,
                            job_id,
                            request_id,
                            ..
                        } = request;
                        match res {
                            Ok(json) => {
                                scheduler_tx.respond_block_request(
                                    &session_id,
                                    BlockResponseParams {
                                        session_id: session_id.clone(),
                                        job_id: job_id.clone(),
                                        error: None,
                                        result: Some(json),
                                        request_id,
                                    },
                                );
                            }
                            Err(err) => {
                                tracing::warn!("{}", err);
                                scheduler_tx.respond_block_request(
                                    &session_id,
                                    BlockResponseParams {
                                        session_id: session_id.clone(),
                                        job_id: job_id.clone(),
                                        result: None,
                                        error: Some(err),
                                        request_id,
                                    },
                                );
                            }
                        }
                    }
                    BlockRequest::QueryDownstream {
                        job_id,
                        outputs,
                        request_id,
                        session_id,
                    } => {
                        let (node, nodes, outputs_def) = {
                            let flow_guard = flow_shared.flow_block.read().unwrap();
                            let node_id = run_flow_ctx
                                .jobs
                                .get(&job_id)
                                .map(|job| job.node_id.to_owned());
                            let node = node_id.and_then(|id| flow_guard.nodes.get(&id).cloned());
                            (node, flow_guard.nodes.clone(), flow_guard.outputs_def.clone())
                        };
                        let res = parse_node_downstream(
                            node.as_ref(),
                            &nodes,
                            &outputs,
                            &outputs_def,
                        );
                        match res {
                            Ok(json) => {
                                scheduler_tx.respond_block_request(
                                    &session_id,
                                    BlockResponseParams {
                                        session_id: session_id.clone(),
                                        job_id: job_id.clone(),
                                        error: None,
                                        result: Some(json),
                                        request_id,
                                    },
                                );
                            }
                            Err(err) => {
                                tracing::warn!("Query downstream failed: {}.", err,);
                                scheduler_tx.respond_block_request(
                                    &session_id,
                                    BlockResponseParams {
                                        session_id: session_id.clone(),
                                        job_id: job_id.clone(),
                                        result: None,
                                        error: Some(err),
                                        request_id,
                                    },
                                );
                            }
                        }
                    }
                    BlockRequest::Preview {
                        job_id,
                        payload,
                        request_id,
                        session_id,
                    } => {
                        let node_id = run_flow_ctx
                            .jobs
                            .get(&job_id)
                            .map(|job| job.node_id.to_owned());
                        if let Some(node_id) = &node_id {
                            let should_forward = {
                                let flow_guard = flow_shared.flow_block.read().unwrap();
                                flow_guard
                                    .forward_previews
                                    .as_ref()
                                    .is_some_and(|p| p.contains(node_id))
                            };
                            if should_forward
                            {
                                reporter.forward_previews(node_id.clone(), &payload);
                                run_flow_ctx.parent_block_status.run_request(
                                    BlockRequest::Preview {
                                        job_id,
                                        payload,
                                        request_id,
                                        session_id,
                                    },
                                );
                            }
                        }
                    }
                    BlockRequest::QueryAuth {
                        session_id,
                        job_id,
                        payload,
                        request_id,
                    } => {
                        if let Some(vault_client) = &*vault_client {
                            let result = parse_oauth_request(&payload, vault_client).await;
                            match result {
                                Ok(res) => {
                                    let json = serde_json::to_value(res).unwrap_or_default();
                                    scheduler_tx.respond_block_request(
                                        &session_id,
                                        BlockResponseParams {
                                            session_id: session_id.clone(),
                                            job_id: job_id.clone(),
                                            error: None,
                                            result: Some(json),
                                            request_id,
                                        },
                                    );
                                }
                                Err(err) => {
                                    tracing::warn!("OAuth request failed: {}.", err);
                                    scheduler_tx.respond_block_request(
                                        &session_id,
                                        BlockResponseParams {
                                            session_id: session_id.clone(),
                                            job_id: job_id.clone(),
                                            result: None,
                                            error: Some(err.to_string()),
                                            request_id,
                                        },
                                    );
                                }
                            }
                        } else {
                            tracing::warn!("OAuth request received but no vault client available.");
                            scheduler_tx.respond_block_request(
                                &session_id,
                                BlockResponseParams {
                                    session_id: session_id.clone(),
                                    job_id: job_id.clone(),
                                    error: Some("Vault client is not available.".to_string()),
                                    result: None,
                                    request_id,
                                },
                            );
                        }
                    }
                    BlockRequest::UpdateNodeWeight {
                        session_id,
                        job_id,
                        node_id,
                        weight,
                        request_id,
                    } => {
                        if let Some(estimation_node_progress) =
                            estimation_node_progress_store.get_mut(&node_id)
                        {
                            total_weight += weight - estimation_node_progress.weight;
                            estimation_progress_sum = estimation_progress_sum
                                - (estimation_node_progress.progress
                                    * estimation_node_progress.weight)
                                + (estimation_node_progress.progress * weight);
                            estimation_node_progress.weight = weight;
                            // after weight changed, we need recalculate the flow progress
                            if let Some(flow_progress) = update_node_progress_fn(
                                estimation_node_progress.progress,
                                estimation_node_progress,
                                true,
                                &mut estimation_progress_sum,
                                total_weight,
                            ) {
                                run_flow_ctx
                                    .parent_block_status
                                    .progress(flow_shared.job_id.to_owned(), flow_progress);
                                reporter.progress(flow_progress);
                            }
                        }

                        scheduler_tx.respond_block_request(
                            &session_id,
                            BlockResponseParams {
                                session_id: session_id.clone(),
                                job_id: job_id.clone(),
                                error: None,
                                result: None,
                                request_id,
                            },
                        );
                    }
                },
                block_status::Status::Done {
                    job_id,
                    result,
                    error,
                    error_detail,
                } => {
                    run_pending_node(job_id.to_owned(), &flow_shared, &mut run_flow_ctx);

                    let success_done = error.is_none();

                    if let Some(job) = run_flow_ctx.jobs.get(&job_id) {
                        let flow_guard = flow_shared.flow_block.read().unwrap();
                        if let Some(node) = flow_guard.nodes.get(&job.node_id) {
                            let node_weight_progress =
                                estimation_node_progress_store.get_mut(&job.node_id);

                            if let Some(node_weight_progress) = node_weight_progress {
                                if success_done {
                                    if let Some(flow_progress) = update_node_progress_fn(
                                        100.0,
                                        node_weight_progress,
                                        false,
                                        &mut estimation_progress_sum,
                                        total_weight,
                                    ) {
                                        run_flow_ctx
                                            .parent_block_status
                                            .progress(flow_shared.job_id.to_owned(), flow_progress);
                                        reporter.progress(flow_progress);
                                    }
                                }
                            }

                            if let Some(tos) = node.to() {
                                for (handle, value) in result.unwrap_or_default().iter() {
                                    if let Some(handle_tos) = tos.get(handle) {
                                        produce_new_value(
                                            value,
                                            handle_tos,
                                            &flow_shared,
                                            &mut run_flow_ctx,
                                            true,
                                            &limit_nodes,
                                            &reporter,
                                            &None,
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // error already handled in the block report
                    if let Some(err) = error {
                        let flow_path_str = flow_shared.flow_block.read().unwrap().path_str.clone();
                        save_flow_cache(
                            &run_flow_ctx.node_input_values,
                            &flow_path_str,
                        );

                        let node_id = run_flow_ctx
                            .jobs
                            .get(&job_id)
                            .map(|job| job.node_id.to_owned());

                        let is_context_run_block = node_id
                            .as_ref()
                            // TODO: use constants for run_block prefix
                            .map(|id| id.starts_with("run_block::"))
                            .unwrap_or(true);

                        if is_context_run_block {
                            // if the error is from a run_block, we just log the error and continue run flow. It's user's responsibility to handle the error and consider whether to continue the flow.
                            continue;
                        }

                        run_flow_ctx.jobs.clear();

                        let node_message = format!(
                            "{} failed",
                            node_id
                                .clone()
                                .map(|n| format!("[node id: {n}]"))
                                .unwrap_or_else(|| format!("job_id: {}", job_id)),
                        );

                        let error_stack = if let Some(detail) = &error_detail {
                            [flow_shared.stacks.vec().clone(), detail.stack.to_owned()].concat()
                        } else {
                            let error_stack = flow_shared.stacks.stack(
                                flow_shared.job_id.to_owned(),
                                flow_path_str.clone(),
                                node_id.unwrap_or_else(|| NodeId::from("unknown_node".to_string())),
                            );
                            error_stack.vec().to_owned()
                        };

                        reporter.done(
                            &Some(node_message.clone()),
                            &Some(ErrorDetail {
                                message: None, // hide message here to avoid log duplication
                                stack: error_stack.clone(),
                            }),
                        );

                        if flow_shared.stacks.is_root() {
                            run_flow_ctx.parent_block_status.finish(
                                flow_shared.job_id.to_owned(),
                                None,
                                Some(format!("flow {} failed.", flow_path_str)),
                                Some(ErrorDetail {
                                    message: Some(err),
                                    stack: error_stack,
                                }),
                            );
                        } else {
                            run_flow_ctx.parent_block_status.finish(
                                flow_shared.job_id.to_owned(),
                                None,
                                Some(err.clone()),
                                Some(ErrorDetail {
                                    message: Some(err),
                                    stack: error_stack,
                                }),
                            );
                        }
                        break;
                    } else if remove_job_and_is_finished(&job_id, &mut run_flow_ctx) {
                        flow_success(&flow_shared, &run_flow_ctx, &reporter);
                        break;
                    }
                }
                block_status::Status::Error { error } => {
                    let flow_path_str = flow_shared.flow_block.read().unwrap().path_str.clone();
                    save_flow_cache(
                        &run_flow_ctx.node_input_values,
                        &flow_path_str,
                    );

                    run_flow_ctx.jobs.clear();
                    run_flow_ctx.parent_block_status.error(error);
                    break;
                }
            };
        }
    });

    Some(BlockJobHandle::new(FlowJobHandle { spawn_handle }))
}

fn remove_job_and_is_finished(job_id: &JobId, run_flow_ctx: &mut RunFlowContext) -> bool {
    run_flow_ctx.jobs.remove(job_id);
    is_finish(run_flow_ctx)
}

fn flow_success(shared: &FlowShared, ctx: &RunFlowContext, reporter: &FlowReporterTx) {
    reporter.done(&None, &None);
    ctx.parent_block_status
        .finish(shared.job_id.to_owned(), None, None, None);
    let flow_path_str = shared.flow_block.read().unwrap().path_str.clone();
    save_flow_cache(&ctx.node_input_values, &flow_path_str);
}

fn run_pending_node(job_id: JobId, flow_shared: &FlowShared, run_flow_ctx: &mut RunFlowContext) {
    if let Some(job_handle) = run_flow_ctx.jobs.get(&job_id) {
        let node_id = job_handle.node_id.to_owned();

        let node_queue = run_flow_ctx
            .node_queue_pool
            .entry(node_id.to_owned())
            .or_default();

        node_queue.jobs.remove(&job_id);

        let flow_guard = flow_shared.flow_block.read().unwrap();
        if let Some(node) = flow_guard.nodes.get(&node_id) {
            let concurrency = node.concurrency();
            let jobs_count = node_queue.jobs.len();
            if jobs_count < concurrency as usize && !node_queue.pending.is_empty() {
                if let Some(job_id) = node_queue.pending.iter().next().cloned() {
                    node_queue.pending.remove(&job_id);
                    run_node(node, flow_shared, run_flow_ctx);
                }
            }
        }
    }
}

fn produce_new_value(
    value: &Arc<OutputValue>,
    handle_tos: &Vec<HandleTo>,
    shared: &FlowShared,
    ctx: &mut RunFlowContext,
    run_next_node: bool,
    limit_nodes: &Option<HashSet<NodeId>>,
    reporter: &FlowReporterTx,
    options: &Option<OutputOptions>,
) {
    for handle_to in handle_tos {
        match handle_to {
            HandleTo::ToNodeInput {
                node_id,
                input_handle,
            } => {
                // only handle when options is some:
                // - if to_node_inputs is not in options(None), skip this handle_to processing
                // - if to_node_inputs is in options: check whether this handle_to is contained in to_node_inputs.
                //      if handle_to is in to_node_input, continue processing
                //      if not, skip this handle_to processing
                if options.as_ref().is_some_and(|op| {
                    op.target.as_ref().is_some_and(|t| {
                        t.to_node.as_ref().is_none_or(|to_nodes| {
                            !to_nodes.contains(&ToNodeInput {
                                node_id: node_id.to_owned(),
                                input_handle: input_handle.to_owned(),
                            })
                        })
                    })
                }) {
                    continue;
                }

                let flow_guard = shared.flow_block.read().unwrap();
                let last_fulfill_count = if let Some(node) = flow_guard.nodes.get(node_id) {
                    ctx.node_input_values.node_pending_fulfill(node)
                } else {
                    0
                };

                // if limit_nodes is Some, output should only send to these nodes
                if limit_nodes
                    .as_ref()
                    .is_some_and(|nodes| !nodes.contains(node_id))
                {
                    // if target node is not in running nodes, we just update cache value so that we can use these value in next run.
                    // Normally, cache values are only cached before the block is executed. This is an exception.
                    ctx.node_input_values.update_serializable_cache_value(
                        node_id,
                        input_handle,
                        Arc::clone(value),
                    );
                    continue;
                }

                ctx.node_input_values
                    .insert(node_id, input_handle, Arc::clone(value));

                if run_next_node {
                    if let Some(node) = flow_guard.nodes.get(node_id) {
                        if ctx.node_input_values.is_node_fulfill(node) {
                            let node_queue =
                                ctx.node_queue_pool.entry(node_id.to_owned()).or_default();
                            if node_queue.jobs.len() < node.concurrency() as usize {
                                run_node(node, shared, ctx);
                            } else {
                                let pending_fulfill =
                                    ctx.node_input_values.node_pending_fulfill(node);
                                // this value is fulfill the node's input again, we need added a pending job to queue.
                                if pending_fulfill > last_fulfill_count {
                                    node_queue.pending.insert(JobId::random());
                                    tracing::info!("node queue ({}) is full, add a pending job. current jobs count: {}, concurrency: {}",node_id,node_queue.jobs.len(),node.concurrency());
                                } else {
                                    tracing::info!(
                                        "Node ({}) has pending jobs; this input event will not trigger again as it did not fulfill more than before.",
                                        node_id
                                    );
                                }
                            }
                        }
                    } else {
                        warn!("node {} not found in flow block", node_id);
                    }
                }
            }
            HandleTo::ToFlowOutput {
                output_handle: flow_output_handle,
            } => {
                // Refer to the logic for handling ToNodeInput
                if options.as_ref().is_some_and(|op| {
                    op.target.as_ref().is_some_and(|t| {
                        t.to_flow.as_ref().is_none_or(|to_outputs| {
                            !to_outputs.contains(&ToFlowOutput {
                                output_handle: flow_output_handle.to_owned(),
                            })
                        })
                    })
                }) {
                    continue;
                }

                reporter.output(value.clone(), flow_output_handle);
                ctx.parent_block_status.output(
                    shared.job_id.to_owned(),
                    Arc::clone(value),
                    flow_output_handle.to_owned(),
                    None, // don't support specify nodes for flow output yet. only support limit whether output to flow output or not
                );
            }
        }
    }
}

fn run_node(node: &Node, shared: &FlowShared, ctx: &mut RunFlowContext) {
    let job_id = JobId::random();
    ctx.node_queue_pool
        .entry(node.node_id().to_owned())
        .or_default()
        .jobs
        .insert(job_id.to_owned());

    let block = if matches!(node, Node::Slot(_)) {
        let node_id = node.node_id();
        shared
            .slot_blocks
            .get(node_id)
            .map(|slot| slot.block())
            .unwrap_or_else(|| node.block())
    } else {
        node.block()
    };

    let block_scope = if matches!(node, Node::Slot(_)) {
        shared
            .slot_blocks
            .get(node.node_id())
            .map(|slot| slot.scope().clone())
            .unwrap_or_else(|| node.scope())
    } else {
        node.scope()
    };

    let runtime_scope = match block_scope {
        BlockScope::Package {
            name,
            path,
            node_id,
            ..
        } => RuntimeScope {
            session_id: shared.scope.session_id.clone(),
            pkg_name: Some(name.clone()),
            path: path.clone(),
            data_dir: shared
                .scope
                .pkg_root
                .join(name)
                .to_string_lossy()
                .to_string(),
            pkg_root: shared.scope.pkg_root.clone(),
            node_id: node_id.clone(),
            enable_layer: layer::feature_enabled(),
            is_inject: node.scope().is_inject(),
        },
        BlockScope::Flow { node_id, .. } => RuntimeScope {
            session_id: shared.scope.session_id.clone(),
            pkg_name: shared.scope.pkg_name.clone(),
            pkg_root: shared.scope.pkg_root.clone(),
            data_dir: shared.scope.data_dir.clone(),
            path: shared.scope.path().to_owned(),
            node_id: node_id.clone(),
            enable_layer: shared.scope.need_layer(),
            is_inject: node.scope().is_inject(),
        },
        BlockScope::Slot { .. } => RuntimeScope {
            session_id: shared.scope.session_id.clone(),
            pkg_name: shared.parent_scope.pkg_name.clone(),
            pkg_root: shared.parent_scope.pkg_root.clone(),
            data_dir: shared.parent_scope.data_dir.clone(),
            path: shared.parent_scope.path().to_owned(),
            node_id: None,
            enable_layer: shared.parent_scope.need_layer(),
            is_inject: node.scope().is_inject(),
        },
    };

    let common_job_params = CommonJobParameters {
        shared: shared.shared.clone(),
        stacks: shared.stacks.stack(
            shared.job_id.to_owned(),
            shared.flow_block.read().unwrap().path_str.to_owned(),
            node.node_id().to_owned(),
        ),
        job_id: job_id.clone(),
        inputs: ctx.node_input_values.take(node),
        block_status: ctx.block_status.clone(),
        scope: runtime_scope,
    };

    let job_params = match block {
        Block::Task(task_block) => JobParams::Task {
            inputs_def: node.inputs_def(),
            outputs_def: node.outputs_def(),
            task_block: task_block.clone(),
            parent_flow: Some(shared.flow_block.clone()),
            timeout: node.timeout(),
            inputs_def_patch: node.inputs_def_patch(),
            common: common_job_params,
        },
        Block::Flow(flow_block) => JobParams::Flow {
            flow_block: flow_block.clone(),
            nodes: None,
            parent_scope: shared.scope.clone(),
            node_value_store: NodeInputValues::new(false),
            slot_blocks: match node {
                Node::Flow(n) => n.slots.clone(),
                _ => None,
            },
            path_finder: shared.path_finder.clone(),
            common: common_job_params,
            vault_client: shared.vault_client.clone(),
        },
        Block::Service(service_block) => JobParams::Service {
            service_block: service_block.clone(),
            parent_flow: Some(shared.flow_block.clone()),
            inputs_def_patch: node.inputs_def_patch(),
            common: common_job_params,
        },
        Block::Slot(slot_block) => JobParams::Slot {
            slot_block: slot_block.clone(),
            common: common_job_params,
        },
        Block::Condition(condition_block) => {
            let output_def = match node {
                Node::Condition(n) => n.output_def.clone(),
                _ => None,
            };
            JobParams::Condition {
                condition_block: condition_block.clone(),
                output_def,
                common: common_job_params,
            }
        }
    };
    tracing::info!("run node {} as job {job_id}", node.node_id());

    if let Some(handle) = run_job(job_params) {
        ctx.jobs.insert(
            job_id,
            BlockInFlowJobHandle {
                node_id: node.node_id().to_owned(),
                _job: handle,
            },
        );
    } else {
        warn!("node: {} has no handle", node.node_id());
    }
}

fn is_finish(ctx: &RunFlowContext) -> bool {
    ctx.jobs.is_empty()
}
