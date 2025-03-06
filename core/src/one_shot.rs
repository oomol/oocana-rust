//! Run flow once and exit.

use job::SessionId;
use manifest_meta::{BlockPathResolver, BlockReader, NodeId};
use std::collections::HashSet;
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::exit;
use std::sync::Arc;
use utils::error::Result;
use uuid::Uuid;

const DEFAULT_PORT: u16 = 47688;

pub fn run_block(
    block_path: &str, broker_address: Option<String>, block_search_paths: Option<String>,
    execution_session: Option<String>, reporter_enable: bool, to_node: Option<String>,
    nodes: Option<String>, input_values: Option<String>,
) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_block_async(
            block_path,
            broker_address,
            block_search_paths,
            execution_session,
            reporter_enable,
            to_node.map(|node| NodeId::new(node)),
            nodes.map(|nodes| {
                nodes
                    .split(',')
                    .map(|node| NodeId::new(node.to_owned()))
                    .collect()
            }),
            input_values,
        ))?;
    exit(0)
}

async fn run_block_async(
    block_path: &str, broker_address: Option<String>, block_search_paths: Option<String>,
    execution_session: Option<String>, reporter_enable: bool, to_node: Option<NodeId>,
    nodes: Option<HashSet<NodeId>>, input_values: Option<String>,
) -> Result<()> {
    let session_id =
        SessionId::new(execution_session.unwrap_or_else(|| Uuid::new_v4().to_string()));

    let addr = match broker_address {
        Some(address) => address.parse().unwrap(),
        None => SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), DEFAULT_PORT),
    };

    let (_scheduler_impl_tx, _scheduler_impl_rx) =
        mainframe_mqtt::scheduler::connect(&addr, session_id.to_owned()).await;

    let (scheduler_tx, scheduler_rx) = mainframe::scheduler::create(
        session_id.to_owned(),
        _scheduler_impl_tx,
        _scheduler_impl_rx,
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
    });

    let block_path_resolver =
        BlockPathResolver::new(env::current_dir().unwrap(), block_search_paths);
    let block_reader = BlockReader::new();

    runtime::run(
        shared,
        block_path,
        block_reader,
        block_path_resolver,
        None,
        to_node,
        nodes,
        input_values,
    )
    .await?;

    delay_abort_handle.await.unwrap();

    scheduler_tx.abort();
    reporter_tx.abort();

    _ = scheduler_handle.await;
    _ = reporter_handle.await;

    Ok(())
}
