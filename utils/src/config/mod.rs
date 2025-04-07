mod app;
mod config;
use std::path::PathBuf;

pub use app::*;

pub fn store_dir() -> Option<PathBuf> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();
    Some(PathBuf::from(global_config.global.store_dir.clone()))
}

pub fn oocana_dir() -> Option<PathBuf> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();

    Some(PathBuf::from(global_config.global.oocana_dir.clone()))
}

pub fn search_paths() -> Option<Vec<String>> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();
    global_config.global.search_paths.clone()
}

pub fn extra_search_path() -> Option<Vec<String>> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();
    global_config
        .run
        .extra
        .as_ref()
        .and_then(|e| e.search_paths.clone())
}

pub fn env_file() -> Option<String> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();
    global_config.global.env_file.clone()
}

pub fn bind_path_file() -> Option<String> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();
    global_config.global.bind_path_file.clone()
}
