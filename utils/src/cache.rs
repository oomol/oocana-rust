use crate::config;
use std::path::PathBuf;

/// 文件内容结构
/// {
///    "flow_path": "cache_file_path"
/// }
pub const CACHE_META_FILE: &str = "cache_meta.json";

fn with_oocana_subpath(subpath: &str) -> Option<PathBuf> {
    config::oocana_dir().map(|mut p| {
        p.push(subpath);
        p
    })
}

pub fn cache_meta_file_path() -> Option<PathBuf> {
    with_oocana_subpath(CACHE_META_FILE)
}

pub fn cache_dir() -> Option<PathBuf> {
    with_oocana_subpath("cache")
}
