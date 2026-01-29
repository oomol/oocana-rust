use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use config::Config;
use dirs::home_dir;

use super::{global_config::GlobalConfig, run_config::RunConfig};
use crate::path::expand_home;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub global: GlobalConfig,
    pub run: RunConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            global: GlobalConfig::default(),
            run: RunConfig::default(),
        }
    }
}

static GLOBAL_CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// Get the global config reference.
/// If config was never loaded, automatically loads default config.
pub fn get_config() -> &'static AppConfig {
    GLOBAL_CONFIG.get_or_init(|| {
        parse_config(None::<PathBuf>).unwrap_or_else(|e| {
            tracing::warn!("Failed to load config: {}. Using default.", e);
            AppConfig::default()
        })
    })
}

/// Parse config from file path without setting global state.
/// Returns default config if file doesn't exist.
fn parse_config<P: AsRef<Path>>(file: Option<P>) -> Result<AppConfig, String> {
    let p: PathBuf = match file {
        Some(p) => p.as_ref().to_path_buf(),
        None => {
            let mut default_path = home_dir().ok_or("Failed to get home dir")?;
            default_path.push("oocana");
            default_path.push("config");
            default_path
        }
    };

    let config_path = if p.is_absolute() {
        p
    } else {
        let expanded = PathBuf::from(expand_home(&p));
        if expanded.is_relative() {
            let mut current_dir = std::env::current_dir()
                .map_err(|e| format!("Failed to get current dir: {:?}", e))?;
            current_dir.push(expanded);
            current_dir
        } else {
            expanded
        }
    };

    if !config_path.with_extension("json").exists()
        && !config_path.with_extension("toml").exists()
        && !config_path.with_extension("json5").exists()
    {
        tracing::info!(
            "No config file found at {:?}, using default config",
            config_path
        );
        return Ok(AppConfig::default());
    }

    Config::builder()
        .add_source(config::File::with_name(&config_path.to_string_lossy()))
        .build()
        .map_err(|e| format!("Failed to load config: {:?}", e))?
        .try_deserialize::<AppConfig>()
        .map_err(|e| format!("Failed to deserialize config: {:?}", e))
}

/// Load config from file and set as global config.
/// Returns a clone of the config for compatibility.
/// If config is already initialized, returns the existing config (ignores file param).
pub fn load_config<P: AsRef<Path>>(file: Option<P>) -> Result<AppConfig, String> {
    // If already initialized, return the existing config
    if let Some(config) = GLOBAL_CONFIG.get() {
        return Ok(config.clone());
    }

    let config = parse_config(file)?;

    // Try to set the config; if another thread beat us, use their config
    let _ = GLOBAL_CONFIG.set(config);
    Ok(GLOBAL_CONFIG.get().unwrap().clone())
}

/// Parse config from file without affecting global state.
/// Useful for testing config parsing or reading configs independently.
pub fn parse_config_standalone<P: AsRef<Path>>(file: Option<P>) -> Result<AppConfig, String> {
    parse_config(file)
}

#[cfg(test)]
pub use tests::parse_config_for_test;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only function to parse config without affecting global state.
    pub fn parse_config_for_test<P: AsRef<Path>>(file: Option<P>) -> Result<AppConfig, String> {
        parse_config(file)
    }
}
