use std::path::Path;

use serde::de::DeserializeOwned;
use utils::error::Result;

use crate::manifest::{FlowBlock, PackageMeta, Service, SlotBlock, TaskBlock};
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

pub fn read_flow(flow_manifest_path: &Path) -> Result<FlowBlock> {
    read_manifest_file::<FlowBlock>(flow_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read flow manifest file {:?}",
                flow_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
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

pub fn read_flow_block(flow_manifest_path: &Path) -> Result<FlowBlock> {
    read_manifest_file::<FlowBlock>(flow_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Flow Block manifest file {:?}",
                flow_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}

pub fn read_slot_block(slot_manifest_path: &Path) -> Result<SlotBlock> {
    read_manifest_file::<SlotBlock>(slot_manifest_path).map_err(|error| {
        utils::error::Error::with_source(
            &format!(
                "Unable to read Slot Block manifest file {:?}",
                slot_manifest_path.clean()
            ),
            Box::new(error),
        )
    })
}
pub fn read_manifest_file<T: DeserializeOwned>(file_path: &Path) -> Result<T> {
    let s = std::fs::read_to_string(file_path).unwrap();

    // Remove Unicode line separator and paragraph separator before parsing since they will cause serde_yaml to fail
    let s = s.replace("\u{2028}", "").replace("\u{2029}", "");

    let yaml_data: T = serde_yaml::from_str(&s)?;
    Ok(yaml_data)
}
