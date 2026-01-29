mod app;
mod global_config;
mod run_config;
pub use app::*;

use std::path::PathBuf;

pub fn default_broker_port() -> u16 {
    47688
}

pub fn store_dir() -> Option<PathBuf> {
    Some(PathBuf::from(&get_config().global.store_dir))
}

pub fn oocana_dir() -> Option<PathBuf> {
    Some(PathBuf::from(&get_config().global.oocana_dir))
}

pub fn registry_store_file() -> Option<PathBuf> {
    use crate::path::expand_home;

    if let Ok(val) = std::env::var("OOMOL_REGISTRY_STORE_FILE") {
        if !val.is_empty() {
            return Some(PathBuf::from(expand_home(&val)));
        }
    }

    Some(PathBuf::from(&get_config().global.registry_store_file))
}

pub fn search_paths() -> Option<Vec<String>> {
    get_config().global.search_paths.clone()
}

pub fn extra_search_path() -> Option<Vec<String>> {
    get_config()
        .run
        .extra
        .as_ref()
        .and_then(|e| e.search_paths.clone())
}

pub fn env_file() -> Option<String> {
    get_config().global.env_file.clone()
}

pub fn bind_path_file() -> Option<String> {
    get_config().global.bind_path_file.clone()
}
