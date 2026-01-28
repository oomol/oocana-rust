use tracing::warn;

use super::manifest_file::{find_oo_yaml, find_oo_yaml_in_dir, find_oo_yaml_without_oo_suffix};
use std::collections::HashMap;
use std::fs::canonicalize;
use std::path::{Path, PathBuf};

pub struct BlockManifestParams<'a> {
    pub block_value: BlockValueType,
    pub file_prefix: &'a str,
    pub block_dir: &'a str,
    pub search_paths: &'a [PathBuf],
    pub working_dir: &'a Path,
    pub pkg_version: &'a HashMap<String, String>,
}

/// TODO: better return error with block type and search path instead of Option, so that we can reporter more specific error.
/// search block manifest in <block_dir>/<block_name>/<file_prefix>.oo.[yaml|yml] in working_dir or search_paths.
pub fn search_block_manifest(params: BlockManifestParams) -> Option<PathBuf> {
    let BlockManifestParams {
        block_value: value,
        file_prefix,
        block_dir,
        search_paths,
        working_dir,
        pkg_version,
    } = params;

    match value {
        BlockValueType::SelfBlock { name: block_name } => {
            let mut self_manifest_path = working_dir.to_path_buf();
            self_manifest_path.pop();
            self_manifest_path.pop();
            self_manifest_path.push(block_dir);
            self_manifest_path.push(block_name);
            find_manifest_yaml_file(&self_manifest_path, file_prefix)
        }
        BlockValueType::Direct { path: block_path } => {
            find_block_manifest_file(BlockSearchParams {
                manifest_path: &PathBuf::from(block_path),
                file_prefix,
                flow_dir: working_dir,
                search_paths,
                manifest_maybe_file: true,
            })
        }
        BlockValueType::Pkg {
            pkg_name,
            block_name,
        } => {
            let manifest_path: PathBuf = if let Some(version) = pkg_version.get(&pkg_name) {
                // Use "{pkg_name}-{version}" as the package directory
                [&format!("{}-{}", pkg_name, version), block_dir, &block_name]
                    .iter()
                    .collect()
            } else {
                warn!(
                    "can't find package version for {}. pkg directory will use {} without version",
                    pkg_name, pkg_name
                );
                [&pkg_name, block_dir, &block_name].iter().collect()
            };
            find_block_manifest_file(BlockSearchParams {
                manifest_path: &manifest_path,
                file_prefix,
                flow_dir: working_dir,
                search_paths,
                manifest_maybe_file: false,
            })
        }
        BlockValueType::AbsPath { path } => {
            // Refer to the handling of RelPath
            let absolute_path = PathBuf::from(path);
            let absolute_file_prefix = absolute_path
                .file_stem()
                .and_then(|f| f.to_str())
                .and_then(|s| s.strip_suffix(".oo"))
                .unwrap_or(file_prefix);
            find_manifest_yaml_file(&absolute_path, absolute_file_prefix)
        }
        BlockValueType::RelPath { path } => {
            let block_manifest_path = working_dir.join(path);
            // 1. The path is a directory ike `/path/to/block_name`.
            //    In this case, we will search it `<manifest>.oo.yaml` or `<manifest>.oo.yml` in this directory.
            // 2. The path is a file, like `/path/to/block_name/xxx.oo.yaml` or `/path/to/block_name/xxx.oo.yml`.
            //    In this case, the the xxx can be any name,  but we still require the file is suffixed with .oo.yaml or .oo.yml.
            // 3. The path is a path without oo.<yaml|yml> suffix, like /path/to/block_name/manifest.
            //    In this case, we will search it with suffix .oo.yaml or .oo.yml.
            let relative_file_prefix = block_manifest_path
                .file_stem()
                .and_then(|f| f.to_str())
                .and_then(|s| s.strip_suffix(".oo"))
                .unwrap_or(file_prefix);
            find_manifest_yaml_file(&block_manifest_path, &relative_file_prefix)
        }
    }
}

struct BlockSearchParams<'a> {
    pub manifest_path: &'a Path, // block directory path, like <pkg_name>/<block_type+'s'><block_name> or <block_name>
    pub file_prefix: &'a str, // file_prefix is the name of the manifest without the suffix (oo.yaml, oo.yml)
    pub flow_dir: &'a Path, // flow directory, oocana will treat flow dir as the pkg_dir. otherwise, oocana will treat flow_dir as the last search path.
    pub search_paths: &'a [PathBuf], // search paths for pkg_dir.
    pub manifest_maybe_file: bool, // if false, manifest is must be a directory.
}

// find <manifest_path>/<file_prefix>.oo.yaml in search_paths or flow_dir.
fn find_block_manifest_file(params: BlockSearchParams) -> Option<PathBuf> {
    let BlockSearchParams {
        manifest_path,
        file_prefix,
        flow_dir,
        search_paths,
        manifest_maybe_file,
    } = params;

    for search_path in search_paths.iter() {
        let candidate_path = search_path.join(manifest_path);
        let file_path = find_oo_yaml_in_dir(&candidate_path, file_prefix);
        if let Some(path) = file_path {
            return canonicalize(&path).ok();
        }
    }

    let candidate_path = flow_dir.join(manifest_path);
    let p = if manifest_maybe_file {
        find_oo_yaml(&candidate_path, file_prefix)
    } else {
        find_oo_yaml_in_dir(&candidate_path, file_prefix)
    };
    match p {
        Some(path) => canonicalize(&path).ok(),
        _ => None,
    }
}

fn find_manifest_yaml_file(file_or_dir_path: &Path, file_prefix: &str) -> Option<PathBuf> {
    let result = find_oo_yaml(file_or_dir_path, file_prefix);
    if result.is_some() {
        return result;
    }

    find_oo_yaml_without_oo_suffix(file_or_dir_path)
}

/// Block value type parsed from a string reference.
///
/// Parsing rules (in order):
/// 1. `self::` prefix → SelfBlock
/// 2. Starts with `/` → AbsPath
/// 3. Starts with `./` or `../` → RelPath
/// 4. Contains `::` → Pkg (package::block)
/// 5. Otherwise → Direct (block name or path)
#[derive(Debug, PartialEq, Eq)]
pub enum BlockValueType {
    /// `self::<block_name>` - block in same package
    SelfBlock { name: String },
    /// `<block_name>` - direct block name or path without special prefix
    Direct { path: String },
    /// `<pkg>::<block>` - block from another package
    Pkg {
        pkg_name: String,
        block_name: String,
    },
    /// `/absolute/path` - absolute filesystem path
    AbsPath { path: String },
    /// `./relative/path` or `../relative/path` - relative filesystem path
    RelPath { path: String },
}

const SELF_BLOCK_PREFIX: &str = "self::";

pub fn calculate_block_value_type(block_value: &str) -> BlockValueType {
    // 1. self:: prefix
    if let Some(name) = block_value.strip_prefix(SELF_BLOCK_PREFIX) {
        return BlockValueType::SelfBlock {
            name: name.to_string(),
        };
    }

    // 2. Absolute path (starts with /)
    if block_value.starts_with('/') {
        return BlockValueType::AbsPath {
            path: block_value.to_string(),
        };
    }

    // 3. Relative path (starts with ./ or ../)
    if block_value.starts_with("./") || block_value.starts_with("../") {
        return BlockValueType::RelPath {
            path: block_value.to_string(),
        };
    }

    // 4. Package reference (contains ::), pkg::block or pkg::service::function -> pkg, block or pkg, service
    let mut parts = block_value.split("::");
    if let (Some(pkg_name), Some(block_name)) = (parts.next(), parts.next()) {
        return BlockValueType::Pkg {
            pkg_name: pkg_name.to_string(),
            block_name: block_name.to_string(),
        };
    }

    // 5. Direct block name or path
    BlockValueType::Direct {
        path: block_value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_block_value_type() {
        assert_eq!(
            calculate_block_value_type("self::block1"),
            BlockValueType::SelfBlock {
                name: "block1".to_string()
            }
        );
        assert_eq!(
            calculate_block_value_type("block1"),
            BlockValueType::Direct {
                path: "block1".to_string()
            }
        );
        assert_eq!(
            calculate_block_value_type("pkg1::block1"),
            BlockValueType::Pkg {
                pkg_name: "pkg1".to_string(),
                block_name: "block1".to_string()
            }
        );
        assert_eq!(
            calculate_block_value_type("/abs/path/block1"),
            BlockValueType::AbsPath {
                path: "/abs/path/block1".to_string()
            }
        );
        assert_eq!(
            calculate_block_value_type("./rel/path/block1"),
            BlockValueType::RelPath {
                path: "./rel/path/block1".to_string()
            }
        );
    }
}
