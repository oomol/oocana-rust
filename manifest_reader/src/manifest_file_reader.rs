use serde::de::DeserializeOwned;
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};
use utils::error::Result;

/// Resolve `filename.oo.yaml` or `filename.oo.yml` file from dir.
pub fn resolve_path_from_dir(dir_path: &Path, filename: &str) -> Option<PathBuf> {
    if dir_path.is_dir() {
        let mut guess_file = dir_path.join(format!("{}.oo.yaml", filename));
        if guess_file.is_file() {
            return Some(guess_file);
        }

        guess_file.set_extension("yml");
        if guess_file.is_file() {
            return Some(guess_file);
        }
    }

    None
}

/// Resolve `filename.oo.yaml` or `filename.oo.yml` file from `**/filename` prefix.
pub fn resolve_path_from_prefix(prefix: &Path) -> Option<PathBuf> {
    let mut os_string = prefix.as_os_str().to_os_string();

    os_string.push(".oo.yaml");

    let mut guess_file: PathBuf = os_string.into();

    if guess_file.is_file() {
        return Some(guess_file);
    }

    guess_file.set_extension("yml");

    if guess_file.is_file() {
        return Some(guess_file);
    }

    None
}

/// Resolve `filename.oo.yaml` or `filename.oo.yml` file from dir or return the path if it is a file.
pub fn resolve_path(file_or_dir_path: &Path, filename: &str) -> Option<PathBuf> {
    if file_or_dir_path.is_file() {
        Some(file_or_dir_path.to_path_buf())
    } else {
        resolve_path_from_dir(file_or_dir_path, filename)
    }
}

pub fn read_manifest_file<T: DeserializeOwned>(file_path: &Path) -> Result<T> {
    let yaml_file = File::open(file_path)?;
    let yaml_reader = BufReader::new(yaml_file);
    let yaml_data: T = serde_yaml::from_reader(yaml_reader)?;
    Ok(yaml_data)
}
