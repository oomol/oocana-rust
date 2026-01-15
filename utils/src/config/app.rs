use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use config::Config;
use lazy_static::lazy_static;
use std::sync::Mutex;

use dirs::home_dir;

use super::{global_config::GlobalConfig, run_config::RunConfig};
use crate::path::expand_home;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub global: GlobalConfig,
    pub run: RunConfig,
}

lazy_static! {
    pub static ref GLOBAL_CONFIG: Mutex<AppConfig> = Mutex::new(AppConfig {
        global: GlobalConfig::default(),
        run: RunConfig::default(),
    });
}

pub fn load_config<P: AsRef<Path>>(file: Option<P>) -> Result<AppConfig, String> {
    let p: PathBuf = match file {
        Some(p) => p.as_ref().to_path_buf(),
        None => {
            let mut default_path = home_dir().ok_or("Failed to get home dir")?;
            default_path.push("oocana");
            default_path.push("config");
            default_path.to_path_buf()
        }
    };

    let config_path = if p.is_absolute() {
        p
    } else {
        // 先展开 ~ 和 $HOME
        let expanded = PathBuf::from(expand_home(&p));

        // 如果展开后还是相对路径，拼接当前目录
        if expanded.is_relative() {
            let mut current_dir = std::env::current_dir()
                .map_err(|e| format!("Failed to get current dir: {:?}", e))?;
            current_dir.push(expanded);
            current_dir
        } else {
            expanded
        }
    };

    // TODO: use config set_default to set default value
    if !config_path.with_extension("json").exists()
        && !config_path.with_extension("toml").exists()
        && !config_path.with_extension("json5").exists()
    {
        let app_config = GLOBAL_CONFIG.lock().unwrap();
        tracing::info!(
            "No config file found at {:?}, return default config",
            config_path
        );
        return Ok(app_config.clone());
    }

    Config::builder()
        .add_source(config::File::with_name(&config_path.to_string_lossy()))
        .build()
        .map_err(|e| format!("Failed to load config: {:?}", e))?
        .try_deserialize::<AppConfig>()
        .map_err(|e| format!("Failed to deserialize config: {:?}", e))
        .map(|config| {
            let mut global_config = GLOBAL_CONFIG.lock().unwrap();
            *global_config = config;
            global_config.clone()
        })
}
