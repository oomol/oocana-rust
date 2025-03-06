use crate::settings;
use std::path::PathBuf;

/// 文件内容结构
/// {
///    "flow_path": "cache_file_path"
/// }
pub const CACHE_META_FILE: &str = "cache_meta.json";

pub fn cache_meta_file_path() -> Option<PathBuf> {
    match settings::oocana_dir() {
        Some(mut path) => {
            path.push(CACHE_META_FILE);
            Some(path)
        }
        None => None,
    }
}

pub fn cache_dir() -> Option<PathBuf> {
    match settings::oocana_dir() {
        Some(mut path) => {
            path.push("cache");
            Some(path)
        }
        None => None,
    }
}
