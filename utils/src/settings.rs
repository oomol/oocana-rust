use std::path::PathBuf;

use dirs::home_dir;

// 全局共享的内容，用于存储一些可以共用的信息。比如 package layer 的信息
pub fn global_dir() -> Option<PathBuf> {
    home_dir().map(|mut h| {
        h.push(".oomol-studio");
        h.push("oocana");
        h
    })
}

// 单次信息，无法跨越多个 flow 共享（目前是容器内共用），比如 flow 的运行日志，flow 的 injection layer 等。
pub fn oocana_dir() -> Option<PathBuf> {
    home_dir().map(|mut h| {
        h.push(".oocana");
        h
    })
}
