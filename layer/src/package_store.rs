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
    let package_path_buf = package_path.to_path_buf();

    with_package_store(|store| {
        if let Some(p) = store.packages.get(&key) {
            if p.version == version && p.validate().is_ok() {
                return Ok(p.clone());
            }
        }

        let layer = PackageLayer::create(
            version,
            None,
            bootstrap.clone(),
            bind_path,
            package_path_buf.clone(),
            envs,
            env_file,
        )?;
        store.packages.insert(key.clone(), layer.clone());
        Ok(layer)
    })
}

pub fn delete_package_layer<P: AsRef<Path>>(package_path: P) -> Result<()> {
    let package_path = package_path.as_ref();
    let key = package_path.to_string_lossy().to_string();

    // 读取当前 store 获取包信息
    let store = load_package_store()?;
    let pkg_layer = store.packages.get(&key).cloned();

    // 收集需要删除的 layers
    let mut delete_layers = vec![];
    if let Some(pkg_layer) = pkg_layer {
        if let Some(layers) = pkg_layer.base_layers {
            delete_layers.extend(layers);
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

    // 收集其他包正在使用的 layers（排除当前要删除的包）
    let mut stored_layers = vec![];
    for (pkg_key, p) in store.packages.iter() {
        if pkg_key != &key {
            if let Some(layers) = &p.base_layers {
                stored_layers.extend(layers.clone());
            }
            if let Some(bootstrap_layer) = &p.bootstrap_layer {
                stored_layers.push(bootstrap_layer.clone());
            }
        }
    }

    // 删除不被其他包使用的 layers
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

    // 使用事务原子性地移除包记录
    with_package_store(|store| {
        store.packages.remove(&key);
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

fn package_store_file(write: bool) -> Result<File> {
    let dir = config::store_dir().ok_or("Failed to get home dir")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;
    let file_path = dir.join(PACKAGE_STORE);

    // 使用 OpenOptions 原子性地创建文件（如果不存在），不截断现有内容
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&file_path)
        .map_err(|e| format!("Failed to open file: {:?}", e))?;

    // 获取锁后检查文件是否为空，如果为空则初始化
    FileExt::lock_exclusive(&file).map_err(|e| format!("Failed to lock file: {:?}", e))?;

    let metadata = file
        .metadata()
        .map_err(|e| format!("Failed to get file metadata: {:?}", e))?;

    if metadata.len() == 0 {
        let writer = std::io::BufWriter::new(&file);
        serde_json::to_writer(writer, &PackageLayerStore::default())
            .map_err(|e| format!("Failed to serialize: {:?}", e))?;
    }

    FileExt::unlock(&file).map_err(|e| format!("Failed to unlock file: {:?}", e))?;
    drop(file);

    // 根据需要重新打开文件
    let f = if write {
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&file_path)
            .map_err(|e| format!("Failed to open file for write: {:?}", e))?
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

/// 事务函数：在持有排他锁的情况下读取、修改、保存 store
/// 确保整个读-改-写过程的原子性，避免并发修改导致的数据丢失
pub fn with_package_store<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut PackageLayerStore) -> Result<R>,
{
    let dir = config::store_dir().ok_or("Failed to get home dir")?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;
    let file_path = dir.join(PACKAGE_STORE);

    // 使用 OpenOptions 以读写模式打开，如果不存在则创建，不截断现有内容
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&file_path)
        .map_err(|e| format!("Failed to open file: {:?}", e))?;

    // 获取排他锁，阻塞直到获得锁
    FileExt::lock_exclusive(&file).map_err(|e| format!("Failed to lock file: {:?}", e))?;

    // 读取现有内容，如果文件为空则使用默认值
    let store: PackageLayerStore = {
        let metadata = file
            .metadata()
            .map_err(|e| format!("Failed to get file metadata: {:?}", e))?;
        if metadata.len() == 0 {
            PackageLayerStore::default()
        } else {
            let reader = std::io::BufReader::new(&file);
            serde_json::from_reader(reader).map_err(|e| {
                let _ = FileExt::unlock(&file);
                format!(
                    "Failed to deserialize package store from {}: {:?}",
                    PACKAGE_STORE, e
                )
            })?
        }
    };

    // 版本迁移检查
    let mut store = if store.version != env!("CARGO_PKG_VERSION")
        && (store.version == "0.2.0" || store.version == "0.1.0")
    {
        PackageLayerStore::default()
    } else {
        store
    };

    // 执行用户的闭包
    let result = match f(&mut store) {
        Ok(r) => r,
        Err(e) => {
            let _ = FileExt::unlock(&file);
            return Err(e);
        }
    };

    // 回写文件：先截断再写入
    use std::io::{Seek, SeekFrom};
    file.set_len(0)
        .map_err(|e| format!("Failed to truncate file: {:?}", e))?;
    (&file)
        .seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to seek file: {:?}", e))?;

    let writer = std::io::BufWriter::new(&file);
    serde_json::to_writer_pretty(writer, &store).map_err(|e| {
        let _ = FileExt::unlock(&file);
        format!("Failed to serialize: {:?}", e)
    })?;

    // 释放锁
    FileExt::unlock(&file).map_err(|e| format!("Failed to unlock file: {:?}", e))?;

    Ok(result)
}

pub fn add_import_package(pkg: &PackageLayer) -> Result<()> {
    let pkg = pkg.clone();
    with_package_store(|store| {
        store
            .packages
            .insert(pkg.package_path.to_string_lossy().to_string(), pkg.clone());
        Ok(())
    })
}

#[allow(dead_code)]
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
    use std::path::PathBuf;

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

    #[test]
    fn test_package_layer_store_serialization() {
        let store = PackageLayerStore::default();
        let json = serde_json::to_string(&store).expect("serialize failed");
        let parsed: PackageLayerStore = serde_json::from_str(&json).expect("deserialize failed");
        assert_eq!(parsed.version, env!("CARGO_PKG_VERSION"));
        assert!(parsed.packages.is_empty());
    }

    #[test]
    fn test_package_layer_store_with_package() {
        let mut store = PackageLayerStore::default();
        let layer = PackageLayer {
            version: Some("1.0.0".to_string()),
            base_layers: None,
            source_layer: "test-source".to_string(),
            bootstrap: None,
            bootstrap_layer: None,
            package_path: PathBuf::from("/test/path"),
        };
        store
            .packages
            .insert("/test/path".to_string(), layer.clone());

        let json = serde_json::to_string_pretty(&store).expect("serialize failed");
        let parsed: PackageLayerStore = serde_json::from_str(&json).expect("deserialize failed");

        assert_eq!(parsed.packages.len(), 1);
        let pkg = parsed
            .packages
            .get("/test/path")
            .expect("package not found");
        assert_eq!(pkg.version, Some("1.0.0".to_string()));
        assert_eq!(pkg.source_layer, "test-source");
    }

    #[test]
    fn test_delete_nonexistent_package() {
        let nonexistent_path = "/nonexistent/path/to/package";
        let result = delete_package_layer(nonexistent_path);
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());
    }

    #[test]
    fn test_package_layer_status_not_in_store() {
        let temp_dir = std::env::temp_dir().join("test_package_status");
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
        std::fs::create_dir_all(&temp_dir).expect("create temp dir failed");

        let package_file = temp_dir.join("package.oo.yaml");
        std::fs::write(&package_file, "version: 0.0.1\n").expect("write package file failed");

        let result = package_layer_status(&temp_dir);
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());
        assert_eq!(result.unwrap(), PackageLayerStatus::NotInStore);

        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_list_package_layers_empty() {
        let result = list_package_layers();
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());
    }

    #[test]
    fn test_add_import_package() {
        let layer = PackageLayer {
            version: Some("1.0.0".to_string()),
            base_layers: None,
            source_layer: "import-test-source".to_string(),
            bootstrap: None,
            bootstrap_layer: None,
            package_path: PathBuf::from("/import/test/path"),
        };

        let result = add_import_package(&layer);
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());

        let store = load_package_store().expect("load store failed");
        assert!(store.packages.contains_key("/import/test/path"));

        // cleanup
        with_package_store(|store| {
            store.packages.remove("/import/test/path");
            Ok(())
        })
        .expect("cleanup failed");
    }
}
