use crate::injection_store::load_injection_store;
use crate::layer;
use crate::ovmlayer::LayerType;
use crate::package_layer::PackageLayer;

use fs2::FileExt;
use manifest_reader::path_finder::find_package_file;
use manifest_reader::reader::read_package;
use manifest_reader::Package;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use std::{collections::HashMap, fs};
use utils::{
    error::{Error, Result},
    settings,
};

static PACKAGE_STORE: &str = "package_store.json";

struct Defer<F: FnOnce()>(Option<F>);
impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

fn package_meta<P: AsRef<Path>>(dir: P) -> Result<Package> {
    let p = find_package_file(dir.as_ref());
    if p.is_none() {
        return Err(Error::new("Failed to resolve package file"));
    }
    return read_package(&p.unwrap());
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
            if p.version == version && p.validate().is_ok() {
                Ok(PackageLayerStatus::Exist)
            } else {
                tracing::debug!("package layer version mismatch or layers not exist");
                Ok(PackageLayerStatus::NotInStore)
            }
        }
        None => Ok(PackageLayerStatus::NotInStore),
    }
}

pub fn get_or_create_package_layer<P: AsRef<Path>>(
    package_path: P, bind_path: Option<HashMap<String, String>>,
) -> Result<PackageLayer> {
    let package_path = package_path.as_ref();
    let pkg = package_meta(package_path)?;
    let version = pkg.version;
    let bootstrap = pkg.scripts.and_then(|s| s.bootstrap);

    let store = load_package_store()?;

    let package = store
        .packages
        .get(&package_path.to_string_lossy().to_string());

    match package {
        Some(p) => {
            if p.version == version && p.validate().is_ok() {
                Ok(p.clone())
            } else {
                let layer = PackageLayer::create(
                    version,
                    None,
                    bootstrap,
                    bind_path,
                    package_path.to_path_buf(),
                )?;
                let mut store = load_package_store()?;
                store
                    .packages
                    .insert(package_path.to_string_lossy().to_string(), layer.clone());
                save_package_store(&store, None)?;
                Ok(layer)
            }
        }
        None => {
            let layer = PackageLayer::create(
                version,
                None,
                bootstrap,
                bind_path,
                package_path.to_path_buf(),
            )?;
            let mut store = load_package_store()?;
            store
                .packages
                .insert(package_path.to_string_lossy().to_string(), layer.clone());
            save_package_store(&store, None)?;
            Ok(layer)
        }
    }
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
    let dir = settings::global_dir().ok_or("Failed to get home dir")?;

    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

    let file_path = dir.join(PACKAGE_STORE);

    if !file_path.exists() {
        let f = File::create(&file_path).map_err(|e| format!("Failed to create file: {:?}", e))?;
        FileExt::lock_exclusive(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;
        let writer = std::io::BufWriter::new(&f);
        let store = PackageLayerStore {
            version: env!("CARGO_PKG_VERSION").to_string(),
            packages: HashMap::new(),
        };
        serde_json::to_writer(writer, &store)
            .map_err(|e| format!("Failed to serialize: {:?}", e))?;
        FileExt::unlock(&f).map_err(|e| format!("Failed to unlock file: {:?}", e))?;
    }

    let f = if write {
        File::create(&file_path).map_err(|e| format!("Failed to open file: {:?}", e))?
    } else {
        File::open(&file_path).map_err(|e| format!("Failed to open file: {:?}", e))?
    };

    Ok(f)
}

pub fn load_package_store() -> Result<PackageLayerStore> {
    let f = package_store_file(false)?;

    let reader = std::io::BufReader::new(&f);
    FileExt::lock_shared(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;

    let _defer = Defer(Some(|| {
        FileExt::unlock(&f)
            .map_err(|e| format!("Failed to unlock file: {:?}", e))
            .unwrap();
    }));

    let store: PackageLayerStore =
        serde_json::from_reader(reader).map_err(|e| format!("Failed to deserialize: {:?}", e))?;

    drop(_defer);

    if store.version == env!("CARGO_PKG_VERSION") {
        return Ok(store);
    }

    // 0.1.0 更新了 server-base，所有 layer 都被清空，干脆删除
    // 0.2.0 版本丢失了部分 layer 信息，也重新生成
    if store.version == "0.2.0" || store.version == "0.1.0" {
        let dir = settings::global_dir().ok_or("Failed to get home dir")?;

        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

        let file_path = dir.join(PACKAGE_STORE);
        fs::remove_file(&file_path).map_err(|e| format!("Failed to remove file: {:?}", e))?;
        return load_package_store();
    }

    return Ok(store);
}

pub fn add_import_package(pkg: &PackageLayer) -> Result<()> {
    let mut store = load_package_store()?;

    store
        .packages
        .insert(pkg.package_path.to_string_lossy().to_string(), pkg.clone());
    save_package_store(&store, None)?;

    Ok(())
}

pub fn save_package_store(store: &PackageLayerStore, f: Option<File>) -> Result<()> {
    let file_exist = f.is_some();

    let f = f.unwrap_or(package_store_file(true)?);

    if file_exist {
        // 如果文件存在，可能前面已经进行过锁操作，这里只尝试加锁
        let _ = FileExt::try_lock_exclusive(&f);
    } else {
        FileExt::lock_exclusive(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;
    }

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
