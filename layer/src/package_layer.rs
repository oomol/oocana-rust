use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::fs::{metadata, File};
use std::path::PathBuf;

use crate::cli::exec;
use crate::layer::{
    create_random_layer, export_layer, import_layer, list_layers, run_script_unmerge,
};
use crate::ovmlayer::{cp_to_layer, BindPath};
use crate::package_store::add_import_package;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utils::error::Result;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct PackageLayerExport {
    layers: Vec<String>,
}

static LAYER_FILENAME: &str = "layers.json";
static PACKAGE_FILENAME: &str = "package-layer.json";

use std::sync::LazyLock;

pub static CACHE_DIR: LazyLock<Vec<&str>> = LazyLock::new(|| {
    vec![
        "/root/.local/share/pnpm/store",
        "/root/.npm",
        "/root/.cache/pip",
    ]
});

/// package layer is a layer that contains the package source code and runtime dependencies.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PackageLayer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_layers: Option<Vec<String>>,
    #[serde(default)]
    pub source_layer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap_layer: Option<String>,
    pub package_path: PathBuf,
}

impl PackageLayer {
    #[instrument(skip_all)]
    pub fn create<P: Into<PathBuf> + Debug>(
        version: Option<String>,
        layers: Option<Vec<String>>,
        bootstrap: Option<String>,
        bind_paths: &[BindPath],
        package_path: P,
        envs: &HashMap<String, String>,
    ) -> Result<Self> {
        let package_path: PathBuf = package_path.into();

        let mut cache_bind_paths: Vec<BindPath> = Vec::new();
        let pkg_path = package_path.to_string_lossy().to_string();

        for cache in &*CACHE_DIR {
            if metadata(cache).is_err() {
                tracing::debug!("cache path: {cache:?} not exist. skip this bind path");
                continue;
            }
            cache_bind_paths.push(BindPath {
                source: cache.to_string(),
                target: cache.to_string(),
            });
        }

        for bind_path in bind_paths {
            if metadata(&bind_path.source).is_err() {
                tracing::warn!("passing bind paths {:?} is not exist", bind_path.source);
                continue;
            }
            cache_bind_paths.push(bind_path.clone());
        }

        let source_layer = create_random_layer()?;
        let cmd = cp_to_layer(&source_layer, &pkg_path, &pkg_path);
        exec(cmd)?;

        let bootstrap_layer = if let Some(bootstrap) = &bootstrap {
            let mut merge_layers = layers.clone().unwrap_or_default();
            merge_layers.push(source_layer.clone());

            let bootstrap_layer = create_random_layer()?;
            merge_layers.push(bootstrap_layer.clone());

            run_script_unmerge(
                &merge_layers,
                &cache_bind_paths,
                &Some(pkg_path),
                &bootstrap,
                envs,
            )?;
            Some(bootstrap_layer)
        } else {
            None
        };

        Ok(Self {
            version,
            base_layers: layers,
            source_layer,
            bootstrap,
            bootstrap_layer,
            package_path,
        })
    }

    pub fn validate(&self) -> Result<()> {
        let mut layers: HashSet<String> = self
            .base_layers
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();

        // 兼容操作，以前没有 source_layer 字段。但是这个字段实际是必须的。
        if self.source_layer.len() > 0 {
            layers.insert(self.source_layer.clone());
        }
        if let Some(bootstrap_layer) = &self.bootstrap_layer {
            layers.insert(bootstrap_layer.clone());
        }

        let list = list_layers(None);

        match list {
            Ok(list) => {
                let diff = diff(layers, list.into_iter().collect());
                if diff.len() > 0 {
                    tracing::debug!("layer not exist: {:?}", diff);
                    return Err(format!("layer not exist: {:?}", diff).into());
                }
                Ok(())
            }
            Err(e) => return Err(e),
        }
    }

    pub fn export(&self, dest: &str) -> Result<()> {
        if metadata(dest).is_err() {
            std::fs::create_dir_all(dest)?;
        }

        let mut layers = self.base_layers.clone().unwrap_or_default();
        layers.push(self.source_layer.clone());
        self.bootstrap_layer
            .as_ref()
            .map(|l| layers.push(l.clone()));

        let mut export_layers = vec![];
        for layer in layers.iter() {
            let export_layer_file = export_layer(layer, dest)?;
            let layer_filename = PathBuf::from(export_layer_file)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            export_layers.push(layer_filename);
        }

        let file = File::create(format!("{}/{}", dest, PACKAGE_FILENAME))?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;

        let file = File::create(format!("{}/{}", dest, LAYER_FILENAME))?;
        let writer = std::io::BufWriter::new(file);
        let export = PackageLayerExport {
            layers: export_layers,
        };
        serde_json::to_writer_pretty(writer, &export)?;

        Ok(())
    }
}

pub fn import_package_layer(package_path: &str, from: &str) -> Result<()> {
    if metadata(from).is_err() {
        return Err(format!("path not exist: {:?}", from).into());
    }

    let package_file_path = format!("{}/{}", from, PACKAGE_FILENAME);
    let package_layer_path = format!("{}/{}", from, LAYER_FILENAME);

    if metadata(&package_file_path).is_err() {
        return Err(format!("package.json not exist: {:?}", package_file_path).into());
    }

    if metadata(&package_layer_path).is_err() {
        return Err(format!("package.layers.json not exist: {:?}", package_layer_path).into());
    }

    let package_file = File::open(&package_file_path)?;
    let reader = std::io::BufReader::new(package_file);
    let mut package: PackageLayer = serde_json::from_reader(reader)?;
    package.package_path = PathBuf::from(package_path);

    let layer_file = File::open(&package_layer_path)?;
    let reader = std::io::BufReader::new(layer_file);
    let export_layers: PackageLayerExport = serde_json::from_reader(reader)?;
    let layer_files = export_layers
        .layers
        .iter()
        .map(|layer| format!("{}/{}", from, layer));

    for import_file in layer_files {
        import_layer(&import_file)?;
    }

    package.validate()?;

    add_import_package(&package)?;

    Ok(())
}

fn diff(a: HashSet<String>, b: HashSet<String>) -> Vec<String> {
    let diff: Vec<_> = a.difference(&b).collect();
    diff.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_diff() {
        let a = vec!["a", "b", "c"];
        let b = vec!["a", "b", "d"];
        let a: std::collections::HashSet<String> = a.iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = b.iter().map(|s| s.to_string()).collect();
        let diff = super::diff(a, b);
        assert_eq!(diff, vec!["c"]);

        let a = vec!["a", "b", "c"];
        let b = vec!["a", "b", "c"];
        let a: std::collections::HashSet<String> = a.iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = b.iter().map(|s| s.to_string()).collect();
        let diff = super::diff(a, b);
        assert_eq!(diff.len(), 0);

        let a = vec!["a", "b", "c"];
        let b = vec!["a", "b", "c", "d"];
        let a: std::collections::HashSet<String> = a.iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = b.iter().map(|s| s.to_string()).collect();
        let diff = super::diff(a, b);
        assert_eq!(diff.len(), 0);
    }
}
