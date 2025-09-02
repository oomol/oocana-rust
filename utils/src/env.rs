use std::{collections::HashMap, path::Path};

pub fn load_env_from_file<P: AsRef<Path>>(file_path: &Option<P>) -> HashMap<String, String> {
    let Some(file_path) = file_path else {
        return HashMap::new();
    };

    let file_path = file_path.as_ref();
    if !file_path.is_file() {
        tracing::warn!("Env file not found: {}", file_path.display());
        return HashMap::new();
    }

    match std::fs::read_to_string(file_path) {
        Ok(content) => content
            .lines()
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            })
            .collect(),
        Err(err) => {
            tracing::error!("Failed to read env file {}: {}", file_path.display(), err);
            HashMap::new()
        }
    }
}

pub static OVMLAYER_LOG_ENV_KEY: &str = "OVMLAYER_LOG";
