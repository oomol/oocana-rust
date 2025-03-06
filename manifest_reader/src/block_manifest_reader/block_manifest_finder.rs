use crate::manifest_file_reader;
use std::path::{Component, Path, PathBuf};

/// Locate a block `*.oo.yaml` file
pub fn resolve_block_manifest_path(
    block_name: &str, prefix: &str, base_dir: &Path, block_search_paths: &Vec<PathBuf>,
) -> Option<PathBuf> {
    if block_name.starts_with("self::") {
        let mut block_manifest_path = base_dir.to_path_buf();
        block_manifest_path.pop();
        block_manifest_path.pop();
        block_manifest_path.push("blocks");
        block_manifest_path.push(&block_name[6..]);

        return resolve_path(&block_manifest_path, prefix);
    }

    let block_manifest_path = Path::new(block_name);
    if block_manifest_path.is_absolute() {
        // pkg_name is an absolute path
        return resolve_path(&block_manifest_path, prefix);
    }

    if block_manifest_path
        .components()
        .all(is_normal_path_component)
    {
        // pkg_name is a package name
        // search block package from paths
        for block_search_path in block_search_paths.iter() {
            let result = search_block_manifest(block_name, block_search_path, prefix);
            if result.is_some() {
                return result;
            }
        }

        let result = search_block_manifest(block_name, base_dir, prefix);
        if result.is_some() {
            return result;
        }

        return None;
    }

    // pkg_name is a relative path
    let block_manifest_path = base_dir.join(block_name);
    resolve_path(&block_manifest_path, prefix)
}

fn search_block_manifest(
    block_name: &str, block_search_path: &Path, prefix: &str,
) -> Option<PathBuf> {
    // block_name is `pkg_name::block_name`
    let parts: Vec<&str> = block_name.split("::").filter(|s| s.len() > 0).collect();

    let sep = "/blocks/";

    // 这种格式是在 applet 中的 block，存在两个 :: 分隔符，最后面一个是 block 的名字。
    if parts.len() == 3 {
        let guess_block_path = block_search_path.join(parts[..2].join(sep));
        return resolve_path(&guess_block_path, prefix);
    }

    if parts.len() > 3 {
        return None;
        // TODO:
        // return Err(Error::new(&format!(
        //     "Invalid block name: {}. Too many `::` separators.",
        //     block_name
        // )));
    }

    let guess_block_path = block_search_path.join(parts.join(&sep));

    resolve_path(&guess_block_path, prefix)
}

/// Resolve block meta file path from file path or dir.
fn resolve_path(file_or_dir_path: &Path, prefix: &str) -> Option<PathBuf> {
    let result = manifest_file_reader::resolve_path(file_or_dir_path, prefix);
    if result.is_some() {
        return result;
    }

    manifest_file_reader::resolve_path_from_prefix(file_or_dir_path)
}

fn is_normal_path_component(component: Component) -> bool {
    match component {
        Component::Normal(_) => true,
        _ => false,
    }
}
