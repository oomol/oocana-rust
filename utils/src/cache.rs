use crate::config;
use std::path::PathBuf;

/// 文件内容结构
/// {
///    "flow_path": "cache_file_path"
/// }
pub const CACHE_META_FILE: &str = "cache_meta.json";

fn with_oocana_subpath(subpath: &str) -> PathBuf {
    let mut p = config::oocana_dir();
    p.push(subpath);
    p
}

pub fn cache_meta_file_path() -> PathBuf {
    with_oocana_subpath(CACHE_META_FILE)
}

pub fn cache_dir() -> PathBuf {
    with_oocana_subpath("cache")
}
