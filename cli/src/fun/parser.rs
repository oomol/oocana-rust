use dirs::home_dir;
use std::path::{Path, PathBuf};

pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if path.starts_with("~") {
        if let Some(home) = home_dir() {
            return home.join(path.strip_prefix("~").unwrap());
        }
    }
    path.to_path_buf()
}
