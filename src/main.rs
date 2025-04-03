#[cfg(not(debug_assertions))]
use human_panic::setup_panic;

#[cfg(debug_assertions)]
extern crate better_panic;

use dirs::home_dir;
#[cfg(feature = "backtrace")]
use std::env;

use config::{Config, File};
use utils::{error::Result, settings::Config as GlobalConfig, settings::update_config};

/// The main entry point of the application.
fn main() -> Result<()> {
    unsafe {
        #[cfg(feature = "backtrace")]
        env::set_var("RUST_BACKTRACE", "1")
    };

    // Human Panic. Only enabled when *not* debugging.
    #[cfg(not(debug_assertions))]
    {
        setup_panic!();
    }

    // Better Panic. Only enabled *when* debugging.
    #[cfg(debug_assertions)]
    {
        better_panic::Settings::debug()
            .most_recent_first(false)
            .lineno_suffix(true)
            .verbosity(better_panic::Verbosity::Full)
            .install();
    }

    let config = if let Some(home_dir) = home_dir() {
        let config_path = home_dir
            .join(".oocana")
            .join("config")
            .to_string_lossy()
            .to_string();
        Config::builder()
            .add_source(File::with_name(&config_path))
            .build()
            .ok()
    } else {
        None
    };

    config.map(|c| {
        c.try_deserialize::<GlobalConfig>()
            .map(|config| {
                update_config(config);
            })
            .unwrap_or_else(|e| {
                eprintln!("Failed to load config: {}", e);
            });
    });

    // Match Commands
    cli::cli_match()?;

    Ok(())
}
