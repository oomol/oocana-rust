use layer::BindPath;

use std::{collections::HashMap, env::temp_dir, io::BufRead};

pub fn env_file() -> String {
    std::env::var("OOCANA_ENV_FILE").unwrap_or_else(|_| "".to_string())
}

pub fn temp_root() -> String {
    std::env::var("OOCANA_TEMP_ROOT").unwrap_or_else(|_| temp_dir().to_string_lossy().to_string())
}

pub fn envs(file: &str) -> HashMap<String, String> {
    let mut envs = HashMap::new();

    if !file.is_empty() {
        let path = std::path::Path::new(&file);
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
            tracing::warn!("env file not found: {file}");
        }
    }

    envs
}

pub fn bind_path_file() -> String {
    std::env::var("OOCANA_BIND_PATH_FILE").unwrap_or_else(|_| "".to_string())
}

pub fn bind_path(bind_paths: &Option<Vec<String>>, bind_path_file: &str) -> Vec<BindPath> {
    let mut bind_path_arg: Vec<BindPath> = vec![];

    if !bind_path_file.is_empty() {
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
