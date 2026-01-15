use crate::layer;
use crate::ovmlayer::{BindPath, LayerType};
use crate::package_layer::PackageLayer;

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::path::Path;
use utils::{config, error::Result};

struct Defer<F: FnOnce()>(Option<F>);
impl<F: FnOnce()> Drop for Defer<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

/// Generate registry key from package_name and version.
/// Format: `package_name@version`
pub fn registry_key(package_name: &str, version: &str) -> String {
    format!("{}@{}", package_name, version)
}

#[derive(Debug, PartialEq, Eq)]
pub enum RegistryLayerStatus {
    NotInStore,
    Exist,
}

pub fn registry_layer_status(package_name: &str, version: &str) -> Result<RegistryLayerStatus> {
    let key = registry_key(package_name, version);
    let store = load_registry_store()?;

    let package = store.packages.get(&key);

    match package {
        Some(p) => {
            if p.version.as_deref() != Some(version) {
                tracing::debug!(
                    "{} layer version mismatch, expected: {:?}, found: {:?}",
                    key,
                    version,
                    p.version
                );
                Ok(RegistryLayerStatus::NotInStore)
            } else {
                match p.validate() {
                    Ok(_) => Ok(RegistryLayerStatus::Exist),
                    Err(e) => {
                        tracing::debug!("{} layer validation failed: {}", key, e);
                        Ok(RegistryLayerStatus::NotInStore)
                    }
                }
            }
        }
        None => Ok(RegistryLayerStatus::NotInStore),
    }
}

pub fn get_or_create_registry_layer<P: AsRef<Path>>(
    package_name: &str,
    version: &str,
    package_path: P,
    bootstrap: Option<String>,
    bind_path: &[BindPath],
    envs: &HashMap<String, String>,
    env_file: &Option<String>,
) -> Result<PackageLayer> {
    let key = registry_key(package_name, version);
    let package_path = package_path.as_ref();

    let mut store = load_registry_store()?;

    // Check if existing package is valid
    if let Some(p) = store.packages.get(&key) {
        if p.version.as_deref() == Some(version) {
            let validation_result = p.validate();
            if validation_result.is_ok() {
                return Ok(p.clone());
            }
            // Invalid layer, will recreate
            tracing::debug!("{} layer validation failed: {:?}, recreating", key, validation_result.unwrap_err());
        }
    }

    // Create new layer
    let layer = PackageLayer::create(
        Some(version.to_string()),
        None,
        bootstrap,
        bind_path,
        package_path.to_path_buf(),
        envs,
        env_file,
    )?;

    // Insert and save
    store.packages.insert(key, layer.clone());
    save_registry_store(&store, None)?;

    Ok(layer)
}

pub fn get_registry_layer(package_name: &str, version: &str) -> Result<Option<PackageLayer>> {
    let key = registry_key(package_name, version);
    let store = load_registry_store()?;

    match store.packages.get(&key) {
        Some(p) if p.validate().is_ok() => Ok(Some(p.clone())),
        _ => Ok(None),
    }
}

pub fn delete_registry_layer(package_name: &str, version: &str) -> Result<()> {
    let key = registry_key(package_name, version);
    let mut store = load_registry_store()?;

    // Remove the package and collect all its layers
    let pkg_layer = store.packages.remove(&key);

    let mut delete_layers = vec![];
    if let Some(pkg_layer) = &pkg_layer {
        // Collect all layers including source_layer
        if let Some(layers) = &pkg_layer.base_layers {
            delete_layers.extend(layers.iter().cloned());
        }
        if let Some(bootstrap_layer) = &pkg_layer.bootstrap_layer {
            delete_layers.push(bootstrap_layer.clone());
        }
        delete_layers.push(pkg_layer.source_layer.clone());
    }

    // Collect all layers still referenced by remaining packages
    let mut stored_layers = vec![];
    for (_, p) in store.packages.iter() {
        if let Some(layers) = &p.base_layers {
            stored_layers.extend(layers.iter().cloned());
        }
        if let Some(bootstrap_layer) = &p.bootstrap_layer {
            stored_layers.push(bootstrap_layer.clone());
        }
        stored_layers.push(p.source_layer.clone());
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

    // Save the modified store
    save_registry_store(&store, None)?;

    Ok(())
}

pub fn list_registry_layers() -> Result<Vec<PackageLayer>> {
    let store = load_registry_store()?;
    let mut layers = vec![];
    for (_, p) in store.packages.iter() {
        layers.push(p.clone());
    }

    Ok(layers)
}

fn registry_store_file(write: bool) -> Result<File> {
    let file_path = config::registry_store_file().ok_or("Failed to get registry store file path")?;

    if let Some(dir) = file_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;
    }

    if !file_path.exists() {
        let f = File::create(&file_path).map_err(|e| format!("Failed to create file: {:?}", e))?;
        FileExt::lock_exclusive(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;
        let writer = std::io::BufWriter::new(&f);
        let store = RegistryLayerStore {
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

pub fn load_registry_store() -> Result<RegistryLayerStore> {
    let f = registry_store_file(false)?;

    FileExt::lock_shared(&f).map_err(|e| format!("Failed to lock file: {:?}", e))?;

    let _defer = Defer(Some(|| {
        FileExt::unlock(&f)
            .map_err(|e| format!("Failed to unlock file: {:?}", e))
            .unwrap();
    }));

    let reader = std::io::BufReader::new(&f);
    let store: RegistryLayerStore = serde_json::from_reader(reader).map_err(|e| {
        format!(
            "Failed to deserialize registry store: {:?}",
            e
        )
    })?;

    drop(_defer);

    // TODO: Implement migration logic when version changes
    if store.version != env!("CARGO_PKG_VERSION") {
        tracing::warn!(
            "Registry store version mismatch: {} != {}, proceeding without migration",
            store.version,
            env!("CARGO_PKG_VERSION")
        );
    }

    Ok(store)
}

/// Save registry store to disk.
///
/// # Lock handling
/// - If `f` is Some: caller must already hold exclusive lock, this function will NOT lock
/// - If `f` is None: this function will open and lock the file exclusively
pub fn save_registry_store(store: &RegistryLayerStore, f: Option<File>) -> Result<()> {
    let (f, should_unlock) = match f {
        Some(file) => {
            // Caller already holds the lock
            (file, false)
        }
        None => {
            // We need to open and lock the file
            let file = registry_store_file(true)?;
            FileExt::lock_exclusive(&file).map_err(|e| format!("Failed to lock file: {:?}", e))?;
            (file, true)
        }
    };

    f.set_len(0)
        .map_err(|e| format!("Failed to truncate registry store file: {:?}", e))?;

    let writer = std::io::BufWriter::new(&f);

    serde_json::to_writer_pretty(writer, store)
        .map_err(|e| format!("Failed to serialize: {:?}", e))?;

    if should_unlock {
        FileExt::unlock(&f).map_err(|e| format!("Failed to unlock file: {:?}", e))?;
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegistryLayerStore {
    pub version: String,
    /// key format: package_name@version. package_name can be @scope/name or name
    pub packages: HashMap<String, PackageLayer>,
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn test_registry_key() {
        assert_eq!(registry_key("my-package", "1.0.0"), "my-package@1.0.0");
        assert_eq!(
            registry_key("@scope/my-package", "2.0.0"),
            "@scope/my-package@2.0.0"
        );
    }

    #[test]
    fn test_load_registry_store() {
        let r = load_registry_store();
        assert!(
            r.is_ok(),
            "load_registry_store failed: {:?}",
            r.unwrap_err()
        );
    }
}
