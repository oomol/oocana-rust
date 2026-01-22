//! Injection layer store management.
//!
//! Write operations use [`with_injection_store`] with exclusive file lock.
//! Read operations are lock-free, atomic rename ensures consistency.

use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use utils::{config, error::Result};

use crate::injection_layer::InjectionLayer;

/// key is package path
type FlowInjectionMap = HashMap<String, InjectionLayer>;

#[derive(Serialize, Deserialize)]
pub struct InjectionStore {
    pub version: String,
    /// key is flow path
    pub flow_injection: HashMap<String, FlowInjectionMap>,
}

// 迁移成全局共用，这样可以更好的管理所有 layer
static INJECTION_STORE_FILE: &str = "injection_store.json";
static INJECTION_STORE_LOCK: &str = "injection_store.lock";

const MAX_READ_RETRIES: usize = 5;
const RETRY_DELAY_MS: u64 = 1000;

pub fn injection_store_path() -> Result<PathBuf> {
    let dir = config::oocana_dir().ok_or("Failed to get home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;
    Ok(dir.join(INJECTION_STORE_FILE))
}

fn load_injection_store_with_retry() -> Result<InjectionStore> {
    let file_path = injection_store_path()?;

    for attempt in 0..MAX_READ_RETRIES {
        match std::fs::read_to_string(&file_path) {
            Ok(content) => {
                match serde_json::from_str::<InjectionStore>(&content) {
                    Ok(store) => return Ok(store),
                    Err(e) if attempt < MAX_READ_RETRIES - 1 => {
                        tracing::debug!(
                            "JSON parse failed (attempt {}): {}, retrying...",
                            attempt + 1,
                            e
                        );
                        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
                        continue;
                    }
                    Err(e) => return Err(format!("Failed to deserialize: {:?}", e).into()),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(InjectionStore {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    flow_injection: HashMap::new(),
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

pub fn load_injection_store() -> Result<InjectionStore> {
    load_injection_store_with_retry()
}

fn save_injection_store_atomic(store: &InjectionStore) -> Result<()> {
    let file_path = injection_store_path()?;
    let dir = file_path.parent().ok_or("Failed to get parent dir")?;
    let temp_path = dir.join(format!("{}.tmp.{}", INJECTION_STORE_FILE, std::process::id()));

    let content = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize: {:?}", e))?;

    std::fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write temp file: {:?}", e))?;

    std::fs::rename(&temp_path, &file_path)
        .map_err(|e| format!("Failed to rename temp file: {:?}", e))?;

    Ok(())
}

/// Execute a read-modify-write transaction on the injection store with exclusive file lock.
pub fn with_injection_store<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut InjectionStore) -> Result<R>,
{
    let dir = config::oocana_dir().ok_or("Failed to get home dir")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

    let lock_path = dir.join(INJECTION_STORE_LOCK);
    let lock_file =
        File::create(&lock_path).map_err(|e| format!("Failed to create lock file: {:?}", e))?;

    lock_file
        .lock_exclusive()
        .map_err(|e| format!("Failed to acquire lock: {:?}", e))?;

    let mut store = load_injection_store()?;
    let result = f(&mut store)?;
    save_injection_store_atomic(&store)?;

    Ok(result)
}

pub fn get_injection_layer<P: AsRef<Path>>(
    flow_path: P,
    package_path: &str,
) -> Option<InjectionLayer> {
    let flow_path = flow_path.as_ref();
    let store = load_injection_store().ok()?;
    let key = flow_path.to_string_lossy().to_string();
    let flow_map = store.flow_injection.get(key.as_str()).cloned();
    flow_map.and_then(|m| m.get(package_path).cloned())
}
