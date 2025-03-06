use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use utils::error::Result;
use walkdir::WalkDir;

#[derive(Deserialize, Debug)]
pub struct BlockMeta {
    pub exec: String,
}

/// Locate a block package within the pipeline root directory
pub struct PkgFinder {
    pipeline_root_path: PathBuf,
    map: HashMap<String, PathBuf>,
}

impl PkgFinder {
    pub fn new(pipeline_root_path: PathBuf) -> Self {
        PkgFinder {
            pipeline_root_path,
            map: HashMap::new(),
        }
    }

    pub fn find_pkg(&mut self, pkg_name: &str) -> Option<&PathBuf> {
        if !self.map.contains_key(pkg_name) {
            let mut pkg_name_paths = pkg_name.split("/").collect::<Vec<&str>>();
            let namespace = pkg_name_paths.remove(0);

            let entries = WalkDir::new(&self.pipeline_root_path.join("packages"))
                .into_iter()
                .filter_entry(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .map(|s| !s.starts_with(".") && s != "node_modules")
                        .unwrap_or(false)
                })
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_type().is_dir()
                        && entry
                            .file_name()
                            .to_str()
                            .map(|s| s == namespace)
                            .unwrap_or(false)
                });

            for entry in entries {
                let mut pkg_path = entry.path().to_path_buf();
                for p in pkg_name_paths.iter() {
                    pkg_path.push(p);
                }
                if pkg_path.join("block.json").is_file() {
                    self.map.insert(pkg_name.to_owned(), pkg_path);
                    break;
                }
            }
        }

        self.map.get(pkg_name)
    }
}
