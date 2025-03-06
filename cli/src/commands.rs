use core::one_shot::run_pipeline;

use utils::app_config::AppConfig;
use utils::error::Result;

/// Show the configuration file
pub fn config() -> Result<()> {
    let config = AppConfig::fetch()?;
    println!("{:#?}", config);

    Ok(())
}

pub fn run(graph_path: &str) -> Result<()> {
    run_pipeline(graph_path)?;

    Ok(())
}
