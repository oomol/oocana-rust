use std::path::Path;
use utils::error::Result;

use crate::Applet;
use manifest_reader::applet_manifest_reader;

pub fn read_applet(applet_path: &Path) -> Result<Applet> {
    let applet_manifest = applet_manifest_reader::read_applet(applet_path)?;
    Ok(Applet::from_manifest(
        applet_manifest,
        applet_path.to_owned(),
    ))
}
