//! Registry layer store management.
//!
//! # Concurrency Model
//!
//! This module implements a **single-writer, multi-reader** concurrency model optimized for
//! NFS compatibility:
//!
//! ## Write Operations (Single Writer Required)
//!
//! Functions that modify the registry store:
//! - [`get_or_create_registry_layer`]
//! - [`delete_registry_layer`]
//! - [`save_registry_store`]
//!
//! **Callers MUST ensure only ONE process executes write operations at any time.**
//! No file locks are used. Concurrent writes will race (last write wins, data loss possible).
//!
//! ## Read Operations (Multi-Reader Safe)
//!
//! Functions that only read the registry store:
//! - [`get_registry_layer`]
//! - [`list_registry_layers`]
//! - [`registry_layer_status`]
//! - [`load_registry_store`]
//!
//! Multiple processes can safely read concurrently, even while a single writer is active.
//! Atomic rename ensures readers always see consistent data (never partial writes).
//!
//! ## Why No File Locks?
//!
//! File locks (flock/fcntl) are unreliable on NFS file systems. Instead, we use:
//! - **Atomic writes**: write-to-temp + rename (POSIX atomic operation)
//! - **Retry on read**: handle transient failures during concurrent read/write
//!
//! This is simpler, more reliable, and works correctly on NFS.

use crate::layer;
use crate::ovmlayer::{BindPath, LayerType};
use crate::package_layer::PackageLayer;

use manifest_reader::path_finder::find_package_file;
use manifest_reader::reader::read_package;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::Path;
use utils::{config, error::Result};

/// Generate registry key from package_name and version.
/// Format: `package_name@version`
pub fn registry_key(package_name: &str, version: &str) -> String {
    format!("{}@{}", package_name, version)
}

const MAX_READ_RETRIES: usize = 5;
const RETRY_DELAY_MS: u64 = 10;

/// Load registry store with retry mechanism for NFS compatibility.
/// Retries on read/parse failures to handle transient issues during atomic writes.
fn load_registry_store_with_retry() -> Result<RegistryLayerStore> {
    let file_path = config::registry_store_file()
        .ok_or("Failed to get registry store file path")?;

    for attempt in 0..MAX_READ_RETRIES {
        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                match serde_json::from_str::<RegistryLayerStore>(&content) {
                    Ok(store) => {
                        // Version check (preserve existing logic)
                        if store.version != env!("CARGO_PKG_VERSION") {
                            tracing::warn!(
                                "Registry store version mismatch: {} != {}, proceeding without migration",
                                store.version,
                                env!("CARGO_PKG_VERSION")
                            );
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
                    Err(e) => return Err(format!("Failed to parse store: {:?}", e).into()),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist, initialize new store
                return Ok(RegistryLayerStore {
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

/// Atomically save registry store using write-to-temp + rename.
/// This is NFS-safe and more reliable than file locks.
fn save_registry_store_atomic(store: &RegistryLayerStore) -> Result<()> {
    let file_path = config::registry_store_file()
        .ok_or("Failed to get registry store file path")?;

    // Ensure directory exists
    if let Some(dir) = file_path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("Failed to create dir: {:?}", e))?;
    }

    // Write to temporary file
    let temp_path = file_path.with_extension("tmp");
    let content = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize: {:?}", e))?;

    std::fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write temp file: {:?}", e))?;

    // Atomic rename (readers see either old or new file, never partial)
    std::fs::rename(&temp_path, &file_path)
        .map_err(|e| format!("Failed to rename temp file: {:?}", e))?;

    Ok(())
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

/// Get or create a registry layer.
///
/// # Concurrency Requirements
///
/// **IMPORTANT: This function performs write operations and assumes a single-writer model.**
///
/// Callers MUST ensure that only ONE process calls this function (or any other registry write
/// function) at the same time. Concurrent writes from multiple processes will result in a
/// race condition where the last write wins, potentially losing data.
///
/// This design choice prioritizes NFS compatibility and simplicity over multi-writer support.
///
/// # Safe Concurrent Usage
///
/// - ✅ Multiple processes can READ concurrently (via `get_registry_layer`, `list_registry_layers`)
/// - ✅ Single process can WRITE while others READ (atomic rename ensures readers see consistent data)
/// - ❌ Multiple processes MUST NOT WRITE concurrently
///
/// Typical usage patterns:
/// - Single mainframe process manages all registry writes
/// - CLI tools only read registry data
/// - If CLI needs to write, ensure mainframe is not running
pub fn get_or_create_registry_layer<P: AsRef<Path>>(
    package_name: &str,
    version: &str,
    package_path: P,
    bind_path: &[BindPath],
    envs: &HashMap<String, String>,
    env_file: &Option<String>,
) -> Result<PackageLayer> {
    let key = registry_key(package_name, version);
    let package_path = package_path.as_ref();

    // Get package metadata to extract bootstrap
    let bootstrap = find_package_file(package_path)
        .and_then(|path| read_package(&path).ok())
        .and_then(|pkg| pkg.scripts)
        .and_then(|scripts| scripts.bootstrap);

    // Load the store with retry mechanism
    let mut store = load_registry_store()?;

    // Check if existing package is valid
    if let Some(p) = store.packages.get(&key) {
        if p.version.as_deref() == Some(version) {
            if p.validate().is_ok() {
                return Ok(p.clone());
            }
            // Invalid layer, will recreate
            tracing::debug!(
                "{} layer validation failed, recreating",
                key
            );
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

    // Insert and save atomically
    store.packages.insert(key, layer.clone());
    save_registry_store_atomic(&store)?;

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

/// Delete a registry layer and its associated OCI layers.
///
/// # Concurrency Requirements
///
/// **IMPORTANT: This is a WRITE operation. Only ONE process can call registry write functions
/// at the same time.**
///
/// See [`get_or_create_registry_layer`] for detailed concurrency requirements.
///
/// # Behavior
///
/// - Removes the package entry from the registry store
/// - Deletes associated OCI layers if they are not shared or in use
/// - Skips deletion of layers that are:
///   - Referenced by other packages in the registry
///   - Currently in use by running containers
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
    let mut delete_errors = vec![];
    for l in delete_layers {
        if stored_layers.contains(&l) {
            tracing::warn!("layer {} is shared, skip delete", l);
        } else if used_layers.contains(&l) {
            tracing::warn!("layer {} is used, skip delete", l);
        } else {
            if let Err(e) = layer::delete_layer(&l) {
                tracing::error!("Failed to delete layer {}: {:?}", l, e);
                delete_errors.push((l, e));
            }
        }
    }

    // Save the modified store regardless of delete errors
    save_registry_store(&store)?;

    // Return error if any deletions failed
    if !delete_errors.is_empty() {
        return Err(format!("Failed to delete {} layer(s)", delete_errors.len()).into());
    }

    Ok(())
}

pub fn list_registry_layers() -> Result<Vec<PackageLayer>> {
    let store = load_registry_store()?;
    Ok(store.packages.into_values().collect())
}

pub fn load_registry_store() -> Result<RegistryLayerStore> {
    load_registry_store_with_retry()
}

/// Save registry store to disk using atomic write.
///
/// # Concurrency Requirements
///
/// **IMPORTANT: This is a WRITE operation. Only ONE process can call registry write functions
/// at the same time.**
///
/// See [`get_or_create_registry_layer`] for detailed concurrency requirements.
///
/// # Implementation
///
/// Uses atomic write (write-to-temp + rename) to ensure:
/// - NFS compatibility (no reliance on file locks)
/// - Readers never see partial/corrupted data
/// - Write operation is all-or-nothing
pub fn save_registry_store(store: &RegistryLayerStore) -> Result<()> {
    save_registry_store_atomic(store)
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
