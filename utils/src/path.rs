use dirs::home_dir;
use std::path::Path;

pub fn to_absolute(p: &Path) -> String {
    if p.is_absolute() {
        p.to_string_lossy().to_string()
    } else {
        match p.canonicalize() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => p.to_string_lossy().to_string(),
        }
    }
}

pub fn expand_home<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();
    if path.starts_with("~") || path.starts_with("$HOME") {
        if let Some(home) = home_dir() {
            let stripped_path = if path.starts_with("~") {
                path.strip_prefix("~").unwrap()
            } else {
                path.strip_prefix("$HOME").unwrap()
            };
            return home.join(stripped_path).to_string_lossy().to_string();
        }
        return path.to_string_lossy().to_string();
    } else {
        return path.to_string_lossy().to_string();
    }
}
