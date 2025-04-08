use super::config::{GlobalConfig, RunConfig};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use config::Config;
use lazy_static::lazy_static;
use std::sync::Mutex;

use dirs::home_dir;

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

    let path = if p.starts_with("~") || p.starts_with("$HOME") {
        let home = home_dir().ok_or_else(|| "Failed to get home dir".to_string())?;
        let stripped_path = if p.starts_with("~") {
            p.strip_prefix("~").unwrap()
        } else {
            p.strip_prefix("$HOME").unwrap()
        };
        let mut home_path = home;
        home_path.push(stripped_path);
        home_path
    } else if p.is_absolute() {
        p.to_path_buf()
    } else if p.is_relative() {
        let mut current_dir =
            std::env::current_dir().map_err(|e| format!("Failed to get current dir: {:?}", e))?;
        current_dir.push(&p);
        current_dir
    } else {
        p.to_path_buf()
    };

    if !path.exists() {
        let app_config = GLOBAL_CONFIG.lock().unwrap();
        return Ok(app_config.clone());
    }

    Config::builder()
        .add_source(config::File::with_name(&path.to_string_lossy()))
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
