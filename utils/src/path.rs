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
