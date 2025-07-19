mod block_job;
pub mod block_status;
pub mod delay_abort;
mod flow_job;
pub mod shared;
use manifest_reader::path_finder::BlockPathFinder;
use std::{
    collections::{HashMap, HashSet},
    env::current_dir,
    path::PathBuf,
    sync::Arc,
};
use tokio::signal::unix::{signal, SignalKind};

use tracing::{error as log_error, info};

use job::{BlockJobStacks, JobId, RuntimeScope};
use manifest_meta::{read_flow_or_block, Block, BlockResolver, MergeInputsValue, NodeId};
use utils::error::Result;

use crate::flow_job::{flow::get_flow_cache_path, NodeInputValues};

const SESSION_CANCEL_INFO: &str = "Cancelled";

pub struct RunArgs<'a> {
    pub shared: Arc<shared::Shared>,
    pub block_name: &'a str,
    pub block_reader: BlockResolver,
    pub path_finder: BlockPathFinder,
    pub job_id: Option<JobId>,
    pub nodes: Option<HashSet<String>>,
    pub input_values: Option<String>,
    pub default_package_path: Option<PathBuf>,
}

pub async fn run(args: RunArgs<'_>) -> Result<()> {
    let RunArgs {
        shared,
        block_name,
        block_reader,
        path_finder,
        job_id,
        nodes,
        input_values,
        default_package_path,
    } = args;
    let (block_status_tx, block_status_rx) = block_status::create();
    let job_id = job_id.unwrap_or_else(JobId::random);
    let stacks = BlockJobStacks::new();
    let partial = nodes.is_some();
    let cache = shared.use_cache;

    let mut block = match read_flow_or_block(block_name, block_reader, path_finder.clone()) {
        Ok(block) => block,
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            // 解析文件失败时，不会运行任何 block。汇报一次 session 开始结束。
            // 错误信息会输出在 stderr 同时 exit code 会以非零状态输出。
            shared.reporter.session_started(block_name, partial, cache);
            shared.reporter.session_finished(
                block_name,
                &Some(format!("Failed to read block {:?}", err)),
                partial,
                cache,
            );
            return Err(err);
        }
    };

    let block_path = block
        .path_str()
        .map(|p| p.to_owned())
        .unwrap_or_else(|| block_name.to_string());

    shared.reporter.session_started(&block_path, partial, cache);

    let nodes = nodes.map(|nodes| nodes.into_iter().map(NodeId::new).collect());

    let scope_workspace = default_package_path
        .filter(|path| path.exists())
        .or_else(|| current_dir().ok());

    let workspace = scope_workspace.expect("workspace not found");

    let flow_cache_path = if let Some(path) = block.path_str() {
        get_flow_cache_path(path)
    } else {
        None
    };

    let node_value_store = match (shared.use_cache, flow_cache_path) {
        (true, Some(cache_path)) => NodeInputValues::recover_from(cache_path, true),
        _ => NodeInputValues::new(true),
    };

    if let Some(patch_value_str) = input_values {
        let merge_inputs_value = serde_json::from_str::<MergeInputsValue>(&patch_value_str)
            .map_err(|e| {
                log_error!("Failed to parse input values: {}", e);
                format!("Invalid input values: {}", e)
            })?;
        if let Block::Flow(flow_block) = block {
            let mut inner_flow_block = (*flow_block).clone();
            inner_flow_block.merge_input_values(merge_inputs_value);
            block = Block::Flow(Arc::new(inner_flow_block));
        }
    }

    let handle = block_job::run_block({
        block_job::RunBlockArgs {
            block,
            shared: Arc::clone(&shared),
            parent_flow: None,
            stacks,
            job_id,
            inputs: None,
            block_status: block_status_tx.clone(),
            node_value_store: Some(node_value_store),
            nodes,
            timeout: None,
            inputs_def_patch: None,
            parent_scope: RuntimeScope {
                pkg_name: None,
                path: workspace.clone(),
                node_id: None,
                enable_layer: false, // current give up layer feature
                is_inject: false,
            },
            scope: RuntimeScope {
                pkg_name: None,
                path: workspace.clone(),
                node_id: None,
                enable_layer: false,
                is_inject: false,
            },
            slot_blocks: None,
            path_finder,
        }
    });

    let signal_handler = tokio::task::spawn(async move {
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = sigint.recv() => {
                log_error!("Received SIGINT");
                block_status_tx.error(SESSION_CANCEL_INFO.to_owned());
            }
            _ = sigterm.recv() => {
                log_error!("Received SIGTERM");
                block_status_tx.error(SESSION_CANCEL_INFO.to_owned());
            }
        }
    });

    let mut result_error: Option<String> = None;
    while let Some(status) = block_status_rx.recv().await {
        match status {
            block_status::Status::Outputs { .. } => {}
            block_status::Status::Output { .. } => {}
            block_status::Status::Request { .. } => {}
            block_status::Status::Done { error, .. } => {
                if let Some(err) = error {
                    result_error = Some(err);
                }
                break;
            }
            block_status::Status::Error { error } => {
                result_error = Some(error);
                break;
            }
        };
    }

    signal_handler.abort();
    shared
        .reporter
        .session_finished(&block_path, &result_error, partial, cache);
    info!(
        "session finished: {}. error: {:?}",
        block_path, result_error
    );

    drop(handle);

    if let Some(err) = result_error {
        return Err(utils::error::Error::new(&err));
    }

    Ok(())
}

pub struct GetPackageArgs<'a> {
    pub block: &'a str,
    pub block_reader: BlockResolver,
    pub path_finder: BlockPathFinder,
    pub nodes: Option<HashSet<String>>,
}

pub fn get_packages(args: GetPackageArgs<'_>) -> Result<HashMap<PathBuf, String>> {
    let GetPackageArgs {
        block,
        block_reader,
        path_finder,
        nodes,
    } = args;

    // TODO: 支持查询特定 node 需要的 packages
    let filter_nodes = nodes.unwrap_or_default();

    let mut packages = vec![];
    match read_flow_or_block(block, block_reader, path_finder) {
        Ok(block) => match block {
            Block::Flow(flow) => {
                flow.nodes
                    .iter()
                    .filter(|node| {
                        filter_nodes.is_empty() || filter_nodes.contains(&node.0.to_string())
                    })
                    .for_each(|node| {
                        if let Some(package_path) = node.1.package_path() {
                            packages.push(package_path);
                        }
                    });
            }
            _ => {
                return Err("wrong block type. except flow get others".into());
            }
        },
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            return Err(err);
        }
    };

    let mut package_layers = HashMap::new();
    packages.iter().for_each(|package| {
        let layers = layer::package_layer_status(package);
        if let Ok(layer::PackageLayerStatus::Exist) = layers {
            package_layers.insert(package.clone(), "true".to_owned());
        } else {
            package_layers.insert(package.clone(), "false".to_owned());
        }
    });

    Ok(package_layers)
}

pub struct FindUpstreamArgs<'a> {
    pub block_name: &'a str,
    pub block_reader: BlockResolver,
    pub path_finder: BlockPathFinder,
    pub use_cache: bool,
    pub nodes: Option<HashSet<String>>,
}

pub fn find_upstream(
    args: FindUpstreamArgs<'_>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let FindUpstreamArgs {
        block_name,
        block_reader,
        use_cache,
        path_finder,
        nodes,
    } = args;

    let block = match read_flow_or_block(block_name, block_reader, path_finder) {
        Ok(block) => block,
        Err(err) => {
            log_error!("Failed to read block: {}", err);
            return Err(err);
        }
    };

    let block_path = block
        .path_str()
        .map(|p| p.to_owned())
        .unwrap_or_else(|| block_name.to_string());

    match block {
        Block::Flow(flow) => {
            let args = block_job::FindUpstreamArgs {
                flow_block: flow,
                use_cache,
                nodes: nodes.map(|nodes| nodes.into_iter().map(NodeId::new).collect()),
            };

            Ok(block_job::find_upstream(args))
        }
        _ => {
            log_error!("Block is not a flow block: {}", block_path);
            Err("wrong block type. except flow get others".into())
        }
    }
}
