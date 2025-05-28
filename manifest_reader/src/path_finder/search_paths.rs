use super::manifest_file::{find_oo_yaml, find_oo_yaml_in_dir, find_oo_yaml_without_suffix};
use std::collections::HashMap;
use std::fs::canonicalize;
use std::path::{Component, Path, PathBuf};

pub struct BlockManifestParams<'a> {
    pub value: &'a str,
    pub base_name: &'a str,
    pub block_dir: &'a str,
    pub search_paths: &'a Vec<PathBuf>,
    pub working_dir: &'a Path,
    pub pkg_version: &'a HashMap<String, String>,
}

/// TODO: better return error with block type and search path instead of Option, so that we can reporter more specific error.
/// search block manifest in <block_dir>/<block_name>/<base_name>.oo.[yaml|yml] in working_dir or search_paths.
pub fn search_block_manifest(params: BlockManifestParams) -> Option<PathBuf> {
    let BlockManifestParams {
        value,
        base_name,
        block_dir,
        search_paths,
        working_dir,
        pkg_version,
    } = params;

    let block_type = get_block_value_type(value);
    match block_type {
        BlockValueType::SelfBlock { name: block_name } => {
            let mut self_manifest_path = working_dir.to_path_buf();
            self_manifest_path.pop();
            self_manifest_path.pop();
            self_manifest_path.push(block_dir);
            self_manifest_path.push(block_name);
            find_manifest_yaml_file(&self_manifest_path, base_name)
        }
        BlockValueType::Direct { path: block_path } => {
            let manifest_path = vec![block_path];
            find_block_manifest_file(BlockSearchParams {
                manifest_path: &manifest_path,
                base_name,
                flow_dir: working_dir,
                search_paths,
                manifest_maybe_file: true,
            })
        }
        BlockValueType::Pkg {
            pkg_name,
            block_name,
        } => {
            let manifest_path = if let Some(ref pkg) = pkg_name {
                if let Some(version) = pkg_version.get(pkg) {
                    // {pkg_name}-{version}
                    let pkg = format!("{}-{}", pkg, version);
                    vec![pkg, block_dir.to_string(), block_name]
                } else {
                    vec![pkg.to_string(), block_dir.to_string(), block_name]
                }
            } else {
                // 按理说，不应该走到这里，因为这种情况应该是直接的 block_name
                vec![block_name]
            };
            find_block_manifest_file(BlockSearchParams {
                manifest_path: &manifest_path,
                base_name,
                flow_dir: working_dir,
                search_paths,
                manifest_maybe_file: false,
            })
        }
        BlockValueType::AbsPath { path } => find_manifest_yaml_file(path.as_ref(), base_name),
        BlockValueType::RelPath { path } => {
            let block_manifest_path = working_dir.join(path);
            find_manifest_yaml_file(&block_manifest_path, base_name)
        }
    }
}

/// Parse block value to package name and block name.
/// return block_name and pkg name(Option).
fn get_block_name_and_pkg(block_value: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = block_value.split("::").filter(|s| !s.is_empty()).collect();

    if parts.len() == 1 {
        (parts[0].to_string(), None)
    } else {
        (parts[1].to_string(), Some(parts[0].to_string()))
    }
}

/// 如果能够把这个处理往上移，可能结构会更清晰
#[derive(Debug, PartialEq, Eq)]
pub enum BlockValueType {
    /// start with self::, like self::<block_name> or self::<service_name>::<block_name>
    SelfBlock {
        name: String, // block name without self:: prefix
    },
    /// <block_name> or manifest file path or a directory contains manifest file.
    /// (not start with / and not start with ./ or ../ and not contains '::')
    Direct {
        path: String, // block name or manifest file path
    },
    /// <pkg_name>::<block_name> or <pkg_name>::<service_name>::<block_name>
    Pkg {
        pkg_name: Option<String>, // package name
        block_name: String,       // block name
    },
    /// absolute path, start with /, path can be manifest file path or a directory contains manifest file.
    AbsPath {
        path: String, // absolute path
    },
    /// relative path, start with ./ or ../ or multiple '.'. path can be manifest file path or a directory contains manifest file.
    RelPath {
        path: String, // relative path
    },
}

const SELF_BLOCK_PREFIX: &str = "self::";

pub fn calculate_block_type_and_name(
    block_value: &str,
) -> (BlockValueType, String, Option<String>) {
    let block_type = get_block_value_type(block_value);
    let (block_name, pkg_name) = get_block_name_and_pkg(block_value);
    (block_type, block_name, pkg_name)
}

pub fn get_block_value_type(block_value: &str) -> BlockValueType {
    if block_value.starts_with(SELF_BLOCK_PREFIX) {
        return BlockValueType::SelfBlock {
            name: block_value[SELF_BLOCK_PREFIX.len()..].to_string(),
        };
    }

    let block_path = Path::new(block_value);
    if block_path.is_absolute() {
        return BlockValueType::AbsPath {
            path: block_value.to_string(),
        };
    }

    // 1. <pkg_name>::<service_name>::<block_name> or <pkg_name>::<block_name>
    // 2. <block_name>
    if block_path.components().all(is_normal_path_component) {
        if block_value.contains("::") {
            let (block_name, pkg_name) = get_block_name_and_pkg(block_value);
            return BlockValueType::Pkg {
                pkg_name,
                block_name,
            };
        } else {
            return BlockValueType::Direct {
                path: block_value.to_owned(),
            };
        }
    }

    BlockValueType::RelPath {
        path: block_value.to_owned(),
    }
}

struct BlockSearchParams<'a> {
    pub manifest_path: &'a Vec<String>, // block directory path, like <pkg_name>/<block_type+'s'><block_name> or <block_name>
    pub base_name: &'a str, // base_name is the name of the manifest without the suffix (oo.yaml, oo.yml)
    pub flow_dir: &'a Path, // flow directory, oocana will treat flow dir as the pkg_dir. otherwise, oocana will treat flow_dir as the last search path.
    pub search_paths: &'a Vec<PathBuf>, // search paths for pkg_dir.
    pub manifest_maybe_file: bool, // if false, manifest is must be a directory.
}

// find <manifest_path>/<base_name>.oo.yaml in search_paths or flow_dir.
fn find_block_manifest_file(params: BlockSearchParams) -> Option<PathBuf> {
    let BlockSearchParams {
        manifest_path,
        base_name,
        flow_dir,
        search_paths,
        manifest_maybe_file,
    } = params;

    for search_path in search_paths.iter() {
        let candidate_path = search_path.join(manifest_path.iter().collect::<PathBuf>());
        let file_path = find_oo_yaml_in_dir(&candidate_path, base_name);
        if let Some(path) = file_path {
            return canonicalize(&path).ok();
        }
    }

    let candidate_path = flow_dir.join(manifest_path.iter().collect::<PathBuf>());
    let p = if manifest_maybe_file {
        find_oo_yaml(&candidate_path, base_name)
    } else {
        find_oo_yaml_in_dir(&candidate_path, base_name)
    };
    match p {
        Some(path) => canonicalize(&path).ok(),
        _ => None,
    }
}

fn find_manifest_yaml_file(file_or_dir_path: &Path, base_name: &str) -> Option<PathBuf> {
    let result = find_oo_yaml(file_or_dir_path, base_name);
    if result.is_some() {
        return result;
    }

    find_oo_yaml_without_suffix(file_or_dir_path)
}

fn is_normal_path_component(component: Component) -> bool {
    matches!(component, Component::Normal(_))
}

#[cfg(test)]
mod tests {
    // use super::*;

    // #[test]
    // fn test_get_block_value_type() {
    //     assert_eq!(
    //         get_block_value_type("self::block1"),
    //         BlockValueType::SelfBlock
    //     );
    //     assert_eq!(get_block_value_type("block1"), BlockValueType::Direct);
    //     assert_eq!(get_block_value_type("pkg1::block1"), BlockValueType::Pkg);
    //     assert_eq!(
    //         get_block_value_type("/abs/path/block1"),
    //         BlockValueType::AbsPath
    //     );
    //     assert_eq!(
    //         get_block_value_type("./rel/path/block1"),
    //         BlockValueType::RelPath
    //     );
    // }
}
