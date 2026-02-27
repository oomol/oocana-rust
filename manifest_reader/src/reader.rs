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

/// Metadata read from `.metadata.oo.json` adjacent to a block manifest.
pub struct BlockMetadata {
    pub hide_source: bool,
    /// Per-block timeout override for remote execution, in seconds.
    pub timeout: Option<u64>,
}

/// Read block metadata from a `.metadata.oo.json` file adjacent to the block manifest.
/// Returns defaults (hide_source=false, timeout=None) if the file doesn't exist or can't be parsed.
pub fn read_block_metadata(block_path: &Path) -> BlockMetadata {
    let Some(dir) = block_path.parent() else {
        return BlockMetadata {
            hide_source: false,
            timeout: None,
        };
    };
    let metadata_path = dir.join(".metadata.oo.json");
    let Ok(content) = std::fs::read_to_string(&metadata_path) else {
        return BlockMetadata {
            hide_source: false,
            timeout: None,
        };
    };

    #[derive(serde::Deserialize)]
    struct RawBlockMetadata {
        #[serde(default)]
        hide_source: bool,
        timeout: Option<u64>,
    }

    serde_json::from_str::<RawBlockMetadata>(&content)
        .map(|m| BlockMetadata {
            hide_source: m.hide_source,
            timeout: m.timeout,
        })
        .unwrap_or(BlockMetadata {
            hide_source: false,
            timeout: None,
        })
}
