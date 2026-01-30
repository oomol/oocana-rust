use dirs::home_dir;
use std::path::Path;

pub fn to_absolute(p: &Path) -> String {
    if p.is_absolute() {
        return p.to_string_lossy().to_string();
    }

    // Try canonicalize first (resolves symlinks and makes absolute)
    if let Ok(canonical) = p.canonicalize() {
        return canonical.to_string_lossy().to_string();
    }

    // Fallback: manually join with current_dir
    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(p).to_string_lossy().to_string();
    }

    // Last resort: return original path (should rarely happen)
    p.to_string_lossy().to_string()
}

pub fn expand_home<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();
    let s = path.to_string_lossy();
    const HOME_VAR: &str = "$HOME";
    if s.starts_with('~') || s.starts_with(HOME_VAR) {
        if let Some(home) = home_dir() {
            let rest = s
                .strip_prefix('~')
                .or_else(|| s.strip_prefix(HOME_VAR))
                .unwrap();
            let rest = rest.trim_start_matches(std::path::is_separator);
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    s.to_string()
}
