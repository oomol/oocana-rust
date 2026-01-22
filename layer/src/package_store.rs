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

    let mut store = load_package_store()?;

    if let Some(p) = store.packages.get(&key) {
        if p.version == version && p.validate().is_ok() {
            return Ok(p.clone());
        }
    }

    let layer = PackageLayer::create(
        version,
        None,
        bootstrap,
        bind_path,
        package_path.to_path_buf(),
        envs,
        env_file,
    )?;
    store.packages.insert(key, layer.clone());
    save_package_store(&store, None)?;
    Ok(layer)
}

pub fn delete_package_layer<P: AsRef<Path>>(package_path: P) -> Result<()> {
    let package_path = package_path.as_ref();
    let mut store = load_package_store()?;

    let key = package_path.to_string_lossy().to_string();
    let pkg_layer = store.packages.remove(key.as_str());

    let mut delete_layers = vec![];
    if let Some(pkg_layer) = pkg_layer {
        if let Some(layers) = pkg_layer.base_layers {
            for l in layers {
                delete_layers.push(l);
            }
        }
        if let Some(bootstrap_layer) = pkg_layer.bootstrap_layer {
            delete_layers.push(bootstrap_layer);
        }
        // TODO: 在这里只能清理当前项目的 injection layer，应该要清理全部项目里面的 injection layers 里对应的 package layer
        let injection_store = load_injection_store()?;
        for (_, flow_injection) in injection_store.flow_injection.iter() {
            for (_, injection_layer) in flow_injection.iter() {
                if injection_layer.package_path == key {
                    delete_layers.push(injection_layer.layer_name.to_owned());
                }
            }
        }
    }

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
    for l in delete_layers {
        if stored_layers.contains(&l) {
            tracing::warn!("layer {} is shared, skip delete", l);
        } else if used_layers.contains(&l) {
            tracing::warn!("layer {} is used, skip delete", l);
        } else {
            layer::delete_layer(&l)?;
        }
    }

    let mut store = load_package_store()?;
    store.packages.remove(key.as_str());

    save_package_store(&store, None)?;

    Ok(())
}

/// TODO: deprecated this function
pub fn delete_all_layer_data() -> Result<()> {
    let mut store = load_package_store()?;
    store.packages.clear();
    save_package_store(&store, None)?;

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

fn package_store_file(write: bool) -> Result<File> {
    let dir = config::store_dir().ok_or("Failed to get home dir")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;
    let file_path = dir.join(PACKAGE_STORE);

    if !file_path.exists() {
        let store = PackageLayerStore::default();
        let content = serde_json::to_string_pretty(&store)
            .map_err(|e| format!("Failed to serialize: {:?}", e))?;
        fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write initial store: {:?}", e))?;
    }

    let f = if write {
        File::create(&file_path).map_err(|e| format!("Failed to open file for write: {:?}", e))?
    } else {
        File::open(&file_path).map_err(|e| format!("Failed to open file: {:?}", e))?
    };

    Ok(f)
}

pub fn load_package_store() -> Result<PackageLayerStore> {
    let f = package_store_file(false)?;

    let reader = std::io::BufReader::new(&f);
    FileExt::lock_shared(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;

    let store: PackageLayerStore = serde_json::from_reader(reader).map_err(|e| {
        let _ = FileExt::unlock(&f);
        format!(
            "Failed to deserialize package store from {}: {:?}",
            PACKAGE_STORE, e
        )
    })?;

    let _ = FileExt::unlock(&f);

    if store.version == env!("CARGO_PKG_VERSION") {
        return Ok(store);
    }

    // 0.1.0 更新了 server-base，所有 layer 都被清空，干脆删除
    // 0.2.0 版本丢失了部分 layer 信息，也重新生成
    if store.version == "0.2.0" || store.version == "0.1.0" {
        let dir = config::store_dir().ok_or("Failed to get home dir")?;

        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

        let file_path = dir.join(PACKAGE_STORE);
        fs::remove_file(&file_path).map_err(|e| format!("Failed to remove file: {:?}", e))?;
        return load_package_store();
    }

    Ok(store)
}

pub fn add_import_package(pkg: &PackageLayer) -> Result<()> {
    let mut store = load_package_store()?;

    store
        .packages
        .insert(pkg.package_path.to_string_lossy().to_string(), pkg.clone());
    save_package_store(&store, None)?;

    Ok(())
}

pub fn save_package_store(store: &PackageLayerStore, file: Option<File>) -> Result<()> {
    let file_provided = file.is_some();

    let f = file.unwrap_or(package_store_file(true)?);

    if file_provided {
        // 如果调用者提供了文件句柄，可能已经进行过锁操作，这里只尝试加锁
        let _ = FileExt::try_lock_exclusive(&f);
    } else {
        FileExt::lock_exclusive(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;
    }

    f.set_len(0)
        .map_err(|e| format!("Failed to truncate package store file: {:?}", e))?;

    let writer = std::io::BufWriter::new(&f);

    serde_json::to_writer_pretty(writer, store)
        .map_err(|e| format!("Failed to serialize: {:?}", e))?;

    FileExt::unlock(&f).map_err(|e| format!("Failed to unlock file: {:?}", e))?;

    Ok(())
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

impl Default for PackageLayerStore {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            packages: HashMap::new(),
        }
    }
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
