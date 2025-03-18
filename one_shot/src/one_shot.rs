//! Run flow once and exit.

use job::SessionId;
use mainframe::BindPath;
use manifest_meta::BlockResolver;
use manifest_reader::path_finder::BlockPathFinder;
use std::collections::HashSet;
use std::env;
use std::fs::{self, metadata};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use utils::error::Result;

const DEFAULT_PORT: u16 = 47688;

pub fn run_block(run_args: BlockArgs) -> Result<()> {
    let r = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_block_async(run_args));
    if let Err(err) = r {
        tracing::error!("{err:?}");
        exit(-1);
    }
    exit(0)
}

pub fn run_with_runtime<F, T>(func: F) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(func)
}

pub struct UpstreamArgs<'a> {
    pub block_path: &'a str,
    pub block_search_paths: Option<Vec<PathBuf>>,
    pub use_cache: bool,
    pub nodes: Option<HashSet<String>>,
}

// TODO: 从 one_shot 中移除，这里不需要配置很多环境，简单裹一层意义不大。
pub fn find_upstream<'a>(
    args: UpstreamArgs<'a>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let UpstreamArgs {
        block_path,
        block_search_paths,
        use_cache,
        nodes,
    } = args;

    let block_reader = BlockResolver::new();
    let block_path_finder = BlockPathFinder::new(env::current_dir().unwrap(), block_search_paths);

    let upstream_args = runtime::FindUpstreamArgs {
        block_name: block_path,
        block_reader: block_reader,
        path_finder: block_path_finder,
        use_cache,
        nodes,
    };

    runtime::find_upstream(upstream_args)
}

pub struct BlockArgs<'a> {
    pub block_path: &'a str,
    pub broker_address: Option<String>,
    pub block_search_paths: Option<Vec<PathBuf>>,
    pub session: String,
    pub reporter_enable: bool,
    pub use_cache: bool,
    pub nodes: Option<HashSet<String>>,
    pub input_values: Option<String>,
    pub default_package: Option<String>,
    pub exclude_packages: Option<Vec<String>>,
    pub session_dir: Option<String>,
    pub bind_paths: Vec<BindPath>,
    pub retain_env_keys: Option<Vec<String>>,
    pub env_files: Option<Vec<String>>,
}

async fn run_block_async(block_args: BlockArgs<'_>) -> Result<()> {
    let BlockArgs {
        block_path,
        broker_address,
        block_search_paths,
        session,
        reporter_enable,
        use_cache,
        nodes,
        input_values,
        default_package,
        exclude_packages,
        bind_paths,
        session_dir,
        retain_env_keys,
        env_files,
    } = block_args;
    let session_id = SessionId::new(session);
    tracing::info!("Session {} started", session_id);

    let addr = match broker_address {
        Some(address) => address.parse().unwrap(),
        None => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_PORT),
    };

    let (_scheduler_impl_tx, _scheduler_impl_rx) =
        mainframe_mqtt::scheduler::connect(&addr, session_id.to_owned()).await;

    let block_path_finder = BlockPathFinder::new(env::current_dir().unwrap(), block_search_paths);
    let default_pkg_path = if let Some(ref default_pkg) = default_package {
        block_path_finder.find_package_file_path(default_pkg).ok()
    } else {
        None
    };

    let session_dir = session_dir.unwrap_or(format!(
        "{}/{session_id}",
        env::temp_dir().to_string_lossy()
    ));

    if metadata(&session_dir).is_err() {
        fs::create_dir_all(&session_dir)?;
    }

    let (scheduler_tx, scheduler_rx) = mainframe::scheduler::create(
        session_id.to_owned(),
        addr.to_string(),
        bind_paths,
        _scheduler_impl_tx,
        _scheduler_impl_rx,
        default_pkg_path.and_then(|p| p.to_str().map(|s| s.to_owned())),
        exclude_packages,
        session_dir,
        retain_env_keys.unwrap_or_default(),
        env_files.unwrap_or_default(),
    );
    let scheduler_handle = scheduler_rx.event_loop();

    let (reporter_tx, reporter_rx) = if reporter_enable {
        let (_reporter_impl_tx, _reporter_impl_rx) = mainframe_mqtt::reporter::connect(&addr).await;
        mainframe::reporter::create(
            session_id.to_owned(),
            Some(_reporter_impl_tx),
            Some(_reporter_impl_rx),
        )
    } else {
        mainframe::reporter::create(session_id.to_owned(), None, None)
    };
    let reporter_handle = reporter_rx.event_loop();

    let (delay_abort_tx, delay_abort_rx) = runtime::delay_abort::delay_abort();
    // delay to collect rest loggings
    let delay_abort_handle = delay_abort_rx.run();

    let shared = Arc::new(runtime::shared::Shared {
        session_id: session_id.clone(),
        address: addr.to_string(),
        scheduler_tx: scheduler_tx.clone(),
        delay_abort_tx,
        reporter: reporter_tx.clone(),
        use_cache,
    });

    let block_reader = BlockResolver::new();

    let result = runtime::run(runtime::RunArgs {
        shared,
        block_name: block_path,
        block_reader,
        path_finder: block_path_finder,
        job_id: None,
        nodes,
        input_values,
    })
    .await;

    let abort = delay_abort_handle.await;
    if let Err(err) = abort {
        tracing::error!("Failed to abort delay: {:?}", err);
    }

    scheduler_tx.abort();
    reporter_tx.abort();

    _ = scheduler_handle.await;
    _ = reporter_handle.await;

    tracing::info!("Session {} finished, result: {:?}", session_id, result);

    result
}
