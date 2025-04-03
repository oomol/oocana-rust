use std::path::PathBuf;

use config::Config;
use dirs::home_dir;
use lazy_static::lazy_static;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GlobalConfig {
    pub store_dir: Option<String>,
    pub oocana_dir: Option<String>,
    pub env_file: Option<String>,
    pub bind_path_file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunExtraConfig {
    pub search_path: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunConfig {
    pub search_path: Option<Vec<String>>,
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub global: Option<GlobalConfig>,
    pub run: Option<RunConfig>,
}

lazy_static! {
    pub static ref GLOBAL_CONFIG: Mutex<AppConfig> = Mutex::new(AppConfig {
        global: None,
        run: None,
    });
}

pub fn load_config(file: Option<&str>) -> Result<AppConfig, String> {
    let path = match file {
        Some(p) => p.to_string(),
        None => {
            let mut default_path = home_dir().ok_or("Failed to get home dir")?;
            default_path.push("oocana");
            default_path.push("config");
            default_path
                .to_str()
                .ok_or("Failed to convert path to str")?
                .to_string()
        }
    };

    Config::builder()
        .add_source(config::File::with_name(&path))
        .build()
        .map_err(|e| format!("Failed to load config: {:?}", e))?
        .try_deserialize::<AppConfig>()
        .map_err(|e| format!("Failed to deserialize config: {:?}", e))
        .map(|config| {
            let mut global_config = GLOBAL_CONFIG.lock().unwrap();
            *global_config = config;
            global_config.clone()
        })
        .map_err(|e| format!("Failed to load config: {:?}", e))
}

pub fn store_dir() -> Option<PathBuf> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();

    let store_dir = global_config
        .global
        .as_ref()
        .and_then(|g| g.store_dir.as_ref())
        .map(|s| PathBuf::from(s));

    match store_dir {
        Some(store_dir) => Some(store_dir),
        None => home_dir().map(|mut h| {
            h.push(".oomol-studio");
            h.push("oocana");
            h
        }),
    }
}

pub fn oocana_dir() -> Option<PathBuf> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();

    let oocana_dir = global_config
        .global
        .as_ref()
        .and_then(|g| g.oocana_dir.as_ref())
        .map(|s| PathBuf::from(s));

    match oocana_dir {
        Some(oocana_dir) => Some(oocana_dir),
        None => home_dir().map(|mut h| {
            h.push(".oocana");
            h
        }),
    }
}
