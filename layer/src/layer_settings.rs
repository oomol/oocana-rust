use serde::{Deserialize, Serialize};
use std::{fs::File, path::PathBuf};
use utils::{
    error::{Error, Result},
    settings,
};

/// 所有 merge layers 时，都会将这个 layers 中的作为 layers 中的最开始部分。
/// {
///    "base_rootfs": ["layer1", "layer2"]
/// }
static LAYER_SETTING: &str = "layer.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct LayerSettings {
    pub base_rootfs: Vec<String>,
}

pub fn layer_setting_file() -> Result<PathBuf> {
    let settings_dir = settings::global_dir().ok_or("Failed to get home dir")?;
    std::fs::create_dir_all(&settings_dir).map_err(|e| format!("Failed to create dir: {:?}", e))?;

    let file = settings_dir.join(LAYER_SETTING);
    if file.exists() {
        return Ok(file);
    } else {
        let f = File::create(&file).map_err(|e| format!("Failed to create file: {:?}", e))?;
        let writer = std::io::BufWriter::new(f);
        let store = LayerSettings {
            base_rootfs: vec![],
        };
        serde_json::to_writer(writer, &store)
            .map_err(|e| format!("Failed to serialize: {:?}", e))?;
    }

    Ok(file)
}

pub fn load_base_rootfs() -> Result<Vec<String>> {
    let path = layer_setting_file()?;

    if let Ok(f) = File::open(path) {
        let reader = std::io::BufReader::new(f);

        match serde_json::from_reader::<_, LayerSettings>(reader) {
            Ok(base) => Ok(base
                .base_rootfs
                .iter()
                .filter(|s| !s.is_empty())
                .cloned()
                .collect()),
            Err(e) => Err(Error::new(&format!(
                "Failed to load_package_store: {:?}",
                e
            ))),
        }
    } else {
        Err(Error::new("Failed to open file"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rootfs_file() {
        let file = layer_setting_file().unwrap();
        assert_eq!(file.exists(), true);
    }

    #[test]
    fn test_load_base_rootfs() {
        let base = load_base_rootfs().unwrap();
        assert_eq!(base.len(), 0);
    }
}
