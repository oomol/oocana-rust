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
    let s = path.to_string_lossy();
    const HOME_VAR: &str = "$HOME";
    if s.starts_with('~') || s.starts_with(HOME_VAR) {
        if let Some(home) = home_dir() {
            let rest = if s.starts_with('~') {
                &s[1..]
            } else {
                &s[HOME_VAR.len()..]
            };
            let rest = rest.trim_start_matches(std::path::is_separator);
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    s.to_string()
}
