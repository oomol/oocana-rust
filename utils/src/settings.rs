use std::path::PathBuf;

use dirs::home_dir;
use lazy_static::lazy_static;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct GlobalConfig {
    pub store_dir: Option<String>,
    pub oocana_dir: Option<String>,
    pub env_file: Option<String>,
    pub bind_path_file: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RunExtraConfig {
    pub search_path: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RunConfig {
    pub search_path: Option<Vec<String>>,
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub global: Option<GlobalConfig>,
    pub run: Option<RunConfig>,
}

lazy_static! {
    pub static ref GLOBAL_CONFIG: Mutex<Config> = Mutex::new(Config {
        global: None,
        run: None,
    });
}

pub fn update_config(config: Config) {
    let mut global_config = GLOBAL_CONFIG.lock().unwrap();
    *global_config = config;
}

pub fn global_dir() -> Option<PathBuf> {
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
