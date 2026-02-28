use std::path::Path;

use serde::de::DeserializeOwned;
use utils::error::Result;

use crate::manifest::{InputHandles, PackageMeta, Service, SubflowBlock, TaskBlock};
use path_clean::PathClean;

pub fn read_task_block(task_manifest_path: &Path) -> Result<TaskBlock> {
    read_manifest_file::<TaskBlock>(task_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Task Block manifest file {:?}",
                task_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

pub fn read_flow(flow_manifest_path: &Path) -> Result<SubflowBlock> {
    read_manifest_file::<SubflowBlock>(flow_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read flow manifest file {:?}",
                flow_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

pub fn read_slotflow(
    inputs_def: Option<InputHandles>,
    slot_manifest_path: &Path,
) -> Result<SubflowBlock> {
    let slotflow = read_manifest_file::<SubflowBlock>(slot_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read slot flow manifest file {:?}",
                slot_manifest_path.clean()
            ),
            Box::new(error),
        )
    });
    let mut slotflow = slotflow?;
    slotflow.inputs_def = inputs_def;
    Ok(slotflow)
}

pub fn read_package<P: AsRef<Path>>(file_path: P) -> Result<PackageMeta> {
    read_manifest_file::<PackageMeta>(file_path.as_ref()).map_err(|err| {
        utils::error::Error::with_source(
            &format!("Unable to read package {:?}", file_path.as_ref().clean()),
            Box::new(err),
        )
    })
}

pub fn read_service(service_manifest_path: &Path) -> Result<Service> {
    read_manifest_file::<Service>(service_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read service manifest file {:?}",
                service_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

pub fn read_flow_block(flow_manifest_path: &Path) -> Result<SubflowBlock> {
    read_manifest_file::<SubflowBlock>(flow_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Flow Block manifest file {:?}",
                flow_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

pub fn read_manifest_file<T: DeserializeOwned>(file_path: &Path) -> Result<T> {
    let s = std::fs::read_to_string(file_path)?;

    // Remove Unicode line separator and paragraph separator before parsing since they will cause serde_yaml to fail
    let s = s.replace("\u{2028}", "").replace("\u{2029}", "");

    let yaml_data: T = serde_yaml::from_str(&s)?;
    Ok(yaml_data)
}

/// Metadata read from `.metadata.oo.json` in the package root directory.
pub struct BlockMetadata {
    pub hide_source: bool,
    /// Per-block timeout override for remote execution, in seconds.
    pub timeout: Option<u64>,
}

/// Read package metadata from a `.metadata.oo.json` file in the package root directory.
/// Returns defaults (hide_source=false, timeout=None) when the metadata file is missing.
/// Falls back to fail-closed (hide_source=true, timeout=None) when metadata exists but cannot be read or parsed.
pub fn read_block_metadata(package_path: &Path) -> BlockMetadata {
    let metadata_path = package_path.join(".metadata.oo.json");
    let content = match std::fs::read_to_string(&metadata_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return BlockMetadata {
                hide_source: false,
                timeout: None,
            };
        }
        Err(err) => {
            tracing::error!(
                "Failed to read block metadata at {:?}: {}. Falling back to hide_source=true.",
                metadata_path,
                err
            );
            return BlockMetadata {
                hide_source: true,
                timeout: None,
            };
        }
    };

    #[derive(serde::Deserialize)]
    struct RawBlockMetadata {
        #[serde(default)]
        hide_source: bool,
        timeout: Option<u64>,
    }

    match serde_json::from_str::<RawBlockMetadata>(&content) {
        Ok(m) => BlockMetadata {
            hide_source: m.hide_source,
            timeout: m.timeout,
        },
        Err(err) => {
            tracing::error!(
                "Invalid block metadata at {:?}: {}. Falling back to hide_source=true.",
                metadata_path,
                err
            );
            BlockMetadata {
                hide_source: true,
                timeout: None,
            }
        }
    }
}
