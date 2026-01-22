//! Package layer store management.
//!
//! Write operations use [`with_package_store`] with exclusive file lock.
//! Read operations are lock-free, atomic rename ensures consistency.

use crate::injection_store::load_injection_store;
use crate::layer;
use crate::ovmlayer::{BindPath, LayerType};
use crate::package_layer::PackageLayer;

use fs2::FileExt;
use manifest_reader::path_finder::find_package_file;
use manifest_reader::reader::read_package;
use manifest_reader::Package;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::File;
use std::path::Path;
use std::{collections::HashMap, fs};
use utils::{
    config,
    error::{Error, Result},
};

static PACKAGE_STORE: &str = "package_store.json";
static PACKAGE_STORE_LOCK: &str = "package_store.lock";

const MAX_READ_RETRIES: usize = 5;
const RETRY_DELAY_MS: u64 = 1000;

fn package_meta<P: AsRef<Path>>(dir: P) -> Result<Package> {
    let p = find_package_file(dir.as_ref());
    match p {
        None => Err(Error::new(&format!(
            "no package file in {}",
            dir.as_ref().display()
        ))),
        Some(path) => read_package(&path),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PackageLayerStatus {
    NotInStore,
    Exist,
}

pub fn package_layer_status<P: AsRef<Path>>(package_path: P) -> Result<PackageLayerStatus> {
    let package_path = package_path.as_ref();
    let pkg = package_meta(package_path)?;
    let version = pkg.version;

    let store = load_package_store()?;

    let package = store
        .packages
        .get(&package_path.to_string_lossy().to_string());

    match package {
        Some(p) => {
            if p.version != version {
                tracing::debug!(
                    "{} layer version mismatch, expected: {:?}, found: {:?}",
                    package_path.display(),
                    version,
                    p.version
                );
                Ok(PackageLayerStatus::NotInStore)
            } else if p.validate().is_ok() {
                Ok(PackageLayerStatus::Exist)
            } else {
                tracing::debug!(
                    "{} layer validation failed: {}",
                    package_path.display(),
                    p.validate().unwrap_err()
                );
                Ok(PackageLayerStatus::NotInStore)
            }
        }
        None => Ok(PackageLayerStatus::NotInStore),
    }
}

pub fn get_or_create_package_layer<P: AsRef<Path>>(
    package_path: P,
    bind_path: &[BindPath],
    envs: &HashMap<String, String>,
    env_file: &Option<String>,
) -> Result<PackageLayer> {
    let package_path = package_path.as_ref();
    let pkg = package_meta(package_path)?;
    let version = pkg.version;
    let bootstrap = pkg.scripts.and_then(|s| s.bootstrap);
    let key = package_path.to_string_lossy().to_string();

    let store = load_package_store()?;
    if let Some(p) = store.packages.get(&key) {
        if p.version == version && p.validate().is_ok() {
            return Ok(p.clone());
        }
    }

    tracing::info!(
        "creating package layer for {}, version: {:?}",
        pkg.name
            .unwrap_or(package_path.to_string_lossy().to_string()),
        version
    );

    let layer = PackageLayer::create(
        version,
        None,
        bootstrap,
        bind_path,
        package_path.to_path_buf(),
        envs,
        env_file,
    )?;

    with_package_store(|store| {
        store.packages.insert(key.clone(), layer.clone());
        Ok(())
    })?;

    Ok(layer)
}

pub fn delete_package_layer<P: AsRef<Path>>(package_path: P) -> Result<()> {
    let package_path = package_path.as_ref();
    let key = package_path.to_string_lossy().to_string();

    let store = load_package_store()?;
    let pkg_layer = store.packages.get(key.as_str());
    let mut pkg_layers_to_delete = vec![];
    if let Some(pkg_layer) = pkg_layer {
        if let Some(layers) = &pkg_layer.base_layers {
            pkg_layers_to_delete.extend(layers.iter().cloned());
        }
        if let Some(bootstrap_layer) = &pkg_layer.bootstrap_layer {
            pkg_layers_to_delete.push(bootstrap_layer.clone());
        }
    }

    let mut stored_layers = vec![];
    for (pkg_key, p) in store.packages.iter() {
        if pkg_key == &key {
            continue;
        }
        if let Some(layers) = &p.base_layers {
            stored_layers.extend(layers.iter().cloned());
        }
        if let Some(bootstrap_layer) = &p.bootstrap_layer {
            stored_layers.push(bootstrap_layer.clone());
        }
    }

    // TODO: 在这里只能清理当前项目的 injection layer，应该要清理全部项目里面的 injection layers 里对应的 package layer
    let mut delete_layers = pkg_layers_to_delete;
    let injection_store = load_injection_store()?;
    for (_, flow_injection) in injection_store.flow_injection.iter() {
        for (_, injection_layer) in flow_injection.iter() {
            if injection_layer.package_path == key {
                delete_layers.push(injection_layer.layer_name.to_owned());
            }
        }
    }

    let used_layers = layer::list_layers(Some(LayerType::UsedLayers))?;
    for l in delete_layers {
        if stored_layers.contains(&l) {
            tracing::warn!("layer {} is shared, skip delete", l);
        } else if used_layers.contains(&l) {
            tracing::warn!("layer {} is used, skip delete", l);
        } else {
            layer::delete_layer(&l)?;
        }
    }

    with_package_store(|store| {
        store.packages.remove(key.as_str());
        Ok(())
    })?;

    Ok(())
}

/// TODO: deprecated this function
pub fn delete_all_layer_data() -> Result<()> {
    with_package_store(|store| {
        store.packages.clear();
        Ok(())
    })?;

    layer::delete_all_layer_data()?;

    Ok(())
}

pub fn list_package_layers() -> Result<Vec<PackageLayer>> {
    let store = load_package_store()?;
    let mut layers = vec![];
    for (_, p) in store.packages.iter() {
        layers.push(p.clone());
    }

    Ok(layers)
}

fn save_package_store_atomic(store: &PackageLayerStore) -> Result<()> {
    let dir = config::store_dir().ok_or("Failed to get home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

    let file_path = dir.join(PACKAGE_STORE);
    let temp_path = dir.join(format!("{}.tmp.{}", PACKAGE_STORE, std::process::id()));

    let content = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize: {:?}", e))?;

    std::fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write temp file: {:?}", e))?;

    std::fs::rename(&temp_path, &file_path)
        .map_err(|e| format!("Failed to rename temp file: {:?}", e))?;

    Ok(())
}

/// Execute a read-modify-write transaction on the package store with exclusive file lock.
pub fn with_package_store<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut PackageLayerStore) -> Result<R>,
{
    let dir = config::store_dir().ok_or("Failed to get store dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

    let lock_path = dir.join(PACKAGE_STORE_LOCK);
    let lock_file =
        File::create(&lock_path).map_err(|e| format!("Failed to create lock file: {:?}", e))?;

    lock_file
        .lock_exclusive()
        .map_err(|e| format!("Failed to acquire lock: {:?}", e))?;

    let mut store = load_package_store()?;
    let result = f(&mut store)?;
    save_package_store_atomic(&store)?;

    Ok(result)
}

fn load_package_store_with_retry() -> Result<PackageLayerStore> {
    let dir = config::store_dir().ok_or("Failed to get home dir")?;
    let file_path = dir.join(PACKAGE_STORE);

    for attempt in 0..MAX_READ_RETRIES {
        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                match serde_json::from_str::<PackageLayerStore>(&content) {
                    Ok(store) => {
                        if store.version == env!("CARGO_PKG_VERSION") {
                            return Ok(store);
                        }

                        // 0.1.0 更新了 server-base，所有 layer 都被清空，干脆删除
                        // 0.2.0 版本丢失了部分 layer 信息，也重新生成
                        if store.version == "0.2.0" || store.version == "0.1.0" {
                            return Ok(PackageLayerStore {
                                version: env!("CARGO_PKG_VERSION").to_string(),
                                packages: HashMap::new(),
                            });
                        }

                        return Ok(store);
                    }
                    Err(e) if attempt < MAX_READ_RETRIES - 1 => {
                        tracing::debug!(
                            "JSON parse failed (attempt {}): {}, retrying...",
                            attempt + 1,
                            e
                        );
                        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                        continue;
                    }
                    Err(e) => {
                        return Err(format!(
                            "Failed to deserialize package store from {}: {:?}",
                            PACKAGE_STORE, e
                        )
                        .into())
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PackageLayerStore {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    packages: HashMap::new(),
                });
            }
            Err(e) if attempt < MAX_READ_RETRIES - 1 => {
                tracing::debug!(
                    "File read failed (attempt {}): {}, retrying...",
                    attempt + 1,
                    e
                );
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                continue;
            }
            Err(e) => return Err(format!("Failed to read file: {:?}", e).into()),
        }
    }

    unreachable!()
}

pub fn load_package_store() -> Result<PackageLayerStore> {
    load_package_store_with_retry()
}

pub fn add_import_package(pkg: &PackageLayer) -> Result<()> {
    with_package_store(|store| {
        store
            .packages
            .insert(pkg.package_path.to_string_lossy().to_string(), pkg.clone());
        Ok(())
    })
}

/// TODO: 移除这个 API，容易误删
#[allow(dead_code)]
pub fn clean_layer_not_in_store() -> Result<()> {
    let store = load_package_store()?;

    let mut stored_layers = vec![];
    for (_, p) in store.packages.iter() {
        if let Some(layers) = &p.base_layers {
            for l in layers {
                stored_layers.push(l.clone());
            }
        }
        if let Some(bootstrap_layer) = &p.bootstrap_layer {
            stored_layers.push(bootstrap_layer.clone());
        }
    }

    let used_layers = layer::list_layers(Some(LayerType::UsedLayers))?;

    let ls = layer::list_layers(None)?;
    let mut delete_layers = vec![];

    let base_layers = crate::layer_settings::load_base_rootfs()?;

    for l in ls {
        if !stored_layers.contains(&l) && !used_layers.contains(&l) && !base_layers.contains(&l) {
            delete_layers.push(l);
        }
    }

    tracing::debug!("delete layers: {:?}", delete_layers);
    for l in delete_layers {
        layer::delete_layer(&l)?;
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PackageLayerStore {
    pub version: String,
    /// key 为 package 的 PATH
    pub packages: HashMap<String, PackageLayer>,
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn test_clean_layers() {
        let r = clean_layer_not_in_store();
        assert!(
            r.is_ok(),
            "clean_layer_not_in_store failed: {:?}",
            r.unwrap_err()
        );
    }

    #[test]
    fn test_load_package_store() {
        let r = load_package_store();
        assert!(r.is_ok(), "load_package_store failed: {:?}", r.unwrap_err());
    }
}
