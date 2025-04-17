use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use utils::{error::Result, config};

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

pub fn injection_store_path() -> Result<PathBuf> {
    let dir = config::oocana_dir().ok_or("Failed to get home dir")?;

    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

    let file = dir.join(INJECTION_STORE_FILE);
    if file.exists() {
        return Ok(file);
    } else {
        let f =
            std::fs::File::create(&file).map_err(|e| format!("Failed to create file: {:?}", e))?;
        let writer = std::io::BufWriter::new(f);
        let store = InjectionStore {
            version: env!("CARGO_PKG_VERSION").to_string(),
            flow_injection: HashMap::new(),
        };
        serde_json::to_writer(writer, &store)
            .map_err(|e| format!("Failed to serialize: {:?}", e))?;
    }

    Ok(file)
}

pub fn load_injection_store() -> Result<InjectionStore> {
    let file = injection_store_path()?;
    let reader = std::io::BufReader::new(std::fs::File::open(&file)?);
    let store: InjectionStore =
        serde_json::from_reader(reader).map_err(|e| format!("Failed to deserialize: {:?}", e))?;

    Ok(store)
}

pub fn get_injection_layer<P: AsRef<Path>>(
    flow_path: P, package_path: &str,
) -> Option<InjectionLayer> {
    let flow_path = flow_path.as_ref();
    let store = load_injection_store().ok()?;
    let key = flow_path.to_string_lossy().to_string();
    let flow_map = store.flow_injection.get(key.as_str()).cloned();
    flow_map.and_then(|m| m.get(package_path).cloned())
}
