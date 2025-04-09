use crate::parser;
use layer::BindPath;
use utils::config;

use std::{collections::HashMap, env::temp_dir, io::BufRead, path::PathBuf};

fn find_env_file() -> Option<String> {
    std::env::var("OOCANA_ENV_FILE").ok()
}

pub fn temp_root() -> String {
    std::env::var("OOCANA_TEMP_ROOT").unwrap_or_else(|_| temp_dir().to_string_lossy().to_string())
}

pub fn config() -> String {
    std::env::var("OOCANA_CONFIG").unwrap_or_else(|_| "~/.oocana/config".to_string())
}

pub fn load_envs(file: &Option<String>) -> HashMap<String, String> {
    let mut envs = HashMap::new();

    let file_path = if let Some(file) = file {
        tracing::debug!("env file found by parameters: {file}");
        file.to_string()
    } else if let Some(file) = find_env_file() {
        tracing::debug!("env file found by OOCANA_ENV_FILE: {file}");
        file
    } else if let Some(file) = utils::config::env_file() {
        tracing::debug!("env file found by config: {file}");
        file
    } else {
        return HashMap::new();
    };

    let path = std::path::Path::new(&file_path);
    if path.is_file() {
        let file = std::fs::File::open(path);

        match file {
            Ok(file) => {
                let reader = std::io::BufReader::new(file);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            let parts = line.split('=').collect::<Vec<&str>>();
                            if parts.len() == 2 {
                                envs.insert(parts[0].to_string(), parts[1].to_string());
                            } else {
                                tracing::warn!("env file line format error: {line}");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("env file read error: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("env file open error: {:?}", e);
            }
        }
    } else {
        tracing::warn!("env file not found: {file_path}");
    }

    envs
}

fn find_bind_path_file() -> Option<String> {
    std::env::var("OOCANA_BIND_PATH_FILE").ok()
}

pub fn load_bind_paths(
    bind_paths: &Option<Vec<String>>,
    bind_path_file: &Option<String>,
) -> Vec<BindPath> {
    let mut bind_path_arg: Vec<BindPath> = vec![];

    let bind_path_file = if let Some(bind_path_file) = bind_path_file {
        Some(bind_path_file.to_string())
    } else if let Some(bind_path_file) = find_bind_path_file() {
        Some(bind_path_file)
    } else if let Some(bind_path_file) = utils::config::bind_path_file() {
        Some(bind_path_file)
    } else {
        None
    };

    if let Some(bind_path_file) = bind_path_file {
        let path = std::path::Path::new(&bind_path_file);
        if path.is_file() {
            let file = std::fs::File::open(path);

            match file {
                Ok(file) => {
                    let reader = std::io::BufReader::new(file);
                    for line in reader.lines() {
                        match line {
                            Ok(line) => {
                                let parts = line.split(':').collect::<Vec<&str>>();
                                if parts.len() == 2 {
                                    bind_path_arg.push(BindPath {
                                        source: parts[0].to_string(),
                                        target: parts[1].to_string(),
                                    });
                                } else {
                                    tracing::warn!("bind path file line format error: {line}");
                                }
                            }
                            Err(e) => {
                                tracing::warn!("bind path file read error: {:?}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("bind path file open error: {:?}", e);
                }
            }
        } else {
            tracing::warn!("bind path file not found: {bind_path_file}");
        }
    }

    if let Some(paths) = bind_paths {
        for path in paths {
            let parts = path.split(':').collect::<Vec<&str>>();
            if parts.len() == 2 {
                bind_path_arg.push(BindPath {
                    source: parts[0].to_string(),
                    target: parts[1].to_string(),
                });
            }
        }
    }

    bind_path_arg
}

pub fn parse_search_paths(search_paths: &Option<String>) -> Option<Vec<PathBuf>> {
    let mut search_paths = if let Some(search_paths) = search_paths {
        Some(
            search_paths
                .split(',')
                .map(|s| parser::expand_tilde(s))
                .collect::<Vec<PathBuf>>(),
        )
    } else if let Some(search_paths) = utils::config::search_paths() {
        Some(
            search_paths
                .iter()
                .map(|s| parser::expand_tilde(s))
                .collect(),
        )
    } else {
        None
    };

    if let Some(ref extra_paths) = config::extra_search_path() {
        if let Some(ref mut paths) = search_paths {
            for extra_path in extra_paths {
                paths.push(parser::expand_tilde(extra_path));
            }
        } else {
            search_paths = Some(
                extra_paths
                    .iter()
                    .map(|s| parser::expand_tilde(s))
                    .collect(),
            );
        }
    }
    search_paths
}
