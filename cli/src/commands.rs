use core::one_shot::run_block;
use utils::app_config::AppConfig;
use utils::error::Result;

/// Show the configuration file
pub fn config() -> Result<()> {
    let config = AppConfig::fetch()?;
    println!("{:#?}", config);

    Ok(())
}

pub fn run(
    block_path: &str, broker_address: Option<String>, block_search_paths: Option<String>,
    execution_session: Option<String>, reporter_enable: bool, to_node: Option<String>,
    nodes: Option<String>, input_values: Option<String>,
) -> Result<()> {
    println!("block_path: {}, broker_address: {:?}, block_search_paths: {:?}, execution_session: {:?}, reporter_enable: {}, to_node: {:?}, nodes: {:?}, input_values: {:?}", 
    block_path, broker_address, block_search_paths, execution_session, reporter_enable, to_node, nodes, input_values);

    run_block(
        block_path,
        broker_address,
        block_search_paths,
        execution_session,
        reporter_enable,
        to_node,
        nodes,
        input_values,
    )
}
