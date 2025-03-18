use std::io::BufRead;

use layer::BindPath;

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
