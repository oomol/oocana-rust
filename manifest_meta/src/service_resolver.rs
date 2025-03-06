use std::path::{Path, PathBuf};
use utils::error::Result;

use crate::Service;
use manifest_reader::reader;

pub fn read_service(service_path: &Path, package_path: Option<PathBuf>) -> Result<Service> {
    let service_manifest = reader::read_service(service_path)?;
    Ok(Service::from_manifest(
        service_manifest,
        service_path.to_owned(),
        package_path,
    ))
}
