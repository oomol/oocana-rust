use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::fs::{File, metadata};
use std::path::PathBuf;

use crate::cli::exec;
use crate::layer::{
    create_random_layer, export_layers, import_layer, list_layers, move_layer, run_script_unmerge,
};
use crate::ovmlayer::{BindPath, cp_to_layer};
use crate::package_store::{add_import_package, get_package_layer, remove_package_from_store};
use manifest_reader::path_finder::find_package_file;
use manifest_reader::reader::read_package;
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
    pub name: Option<String>,
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

fn read_package_name(package_path: &std::path::Path) -> Option<String> {
    find_package_file(package_path)
        .and_then(|package_file| read_package(&package_file).ok())
        .and_then(|pkg| pkg.name)
}

impl PackageLayer {
    pub fn layers(&self) -> Vec<String> {
        let mut layers = self.base_layers.clone().unwrap_or_default();
        layers.push(self.source_layer.clone());
        if let Some(bootstrap_layer) = &self.bootstrap_layer {
            layers.push(bootstrap_layer.clone());
        }
        layers
    }

    #[instrument(skip_all)]
    pub fn create<P: Into<PathBuf> + Debug>(
        version: Option<String>,
        layers: Option<Vec<String>>,
        bootstrap: Option<String>,
        bind_paths: &[BindPath],
        package_path: P,
        envs: &HashMap<String, String>,
        env_file: &Option<String>,
    ) -> Result<Self> {
        let package_path: PathBuf = package_path.into();
        let name = read_package_name(&package_path);

        let mut cache_bind_paths: Vec<BindPath> = Vec::new();
        let pkg_path = package_path.to_string_lossy().to_string();
        let pkg_parent_path = package_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"))
            .to_string_lossy()
            .to_string();

        for cache in &*CACHE_DIR {
            if metadata(cache).is_err() {
                tracing::debug!("cache path: {cache:?} not exist. skip this bind path");
                continue;
            }
            cache_bind_paths.push(BindPath::new(cache, cache, false, false));
        }

        for bind_path in bind_paths {
            if metadata(&bind_path.src).is_err() {
                tracing::warn!("passing bind paths {:?} is not exist", bind_path.src);
                continue;
            }
            cache_bind_paths.push(bind_path.clone());
        }

        let source_layer = create_random_layer()?;
        let cmd = cp_to_layer(&source_layer, &pkg_path, &pkg_parent_path);
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
                bootstrap,
                envs,
                env_file,
            )?;
            Some(bootstrap_layer)
        } else {
            None
        };

        Ok(Self {
            name,
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
        if !self.source_layer.is_empty() {
            layers.insert(self.source_layer.clone());
        }
        if let Some(bootstrap_layer) = &self.bootstrap_layer {
            layers.insert(bootstrap_layer.clone());
        }

        let list = list_layers(None);

        match list {
            Ok(list) => {
                let diff = diff(layers, list.into_iter().collect());
                if !diff.is_empty() {
                    tracing::debug!("layer not exist: {:?}", diff);
                    return Err(format!("layer not exist: {diff:?}").into());
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn export(&self, dest: &str) -> Result<()> {
        if metadata(dest).is_err() {
            std::fs::create_dir_all(dest)?;
        }

        let layers_tar = format!("{dest}/layers.tar");

        let mut layers = self.base_layers.clone().unwrap_or_default();
        layers.push(self.source_layer.clone());
        if let Some(l) = self.bootstrap_layer.as_ref() {
            layers.push(l.clone())
        }

        export_layers(&layers, &layers_tar)?;

        let file = File::create(format!("{dest}/{PACKAGE_FILENAME}"))?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;

        let file = File::create(format!("{dest}/{LAYER_FILENAME}"))?;
        let writer = std::io::BufWriter::new(file);
        let export = PackageLayerExport { layers };
        serde_json::to_writer_pretty(writer, &export)?;

        Ok(())
    }
}

pub fn import_package_layer(package_path: &str, export_dir: &str) -> Result<()> {
    if metadata(export_dir).is_err() {
        return Err(format!("path not exist: {export_dir:?}").into());
    }

    let package_file_path = format!("{export_dir}/{PACKAGE_FILENAME}");
    let package_layer_path = format!("{export_dir}/{LAYER_FILENAME}");

    if metadata(&package_file_path).is_err() {
        return Err(format!("package-layer.json not exist: {package_file_path:?}").into());
    }

    if metadata(&package_layer_path).is_err() {
        return Err(format!("layers.json not exist: {package_layer_path:?}").into());
    }

    let package_file = File::open(&package_file_path)?;
    let reader = std::io::BufReader::new(package_file);
    let mut package: PackageLayer = serde_json::from_reader(reader)?;

    let exported_package_path = package.package_path.to_string_lossy().to_string();

    package.package_path = PathBuf::from(package_path);

    let layer_archive_path = format!("{export_dir}/layers.tar");
    // Imported package layers must remain immediately available for runtime lookup
    // and validation after the archive import completes.
    import_layer(&layer_archive_path)?;

    if exported_package_path != package_path {
        // because layer's path is relative to package path , is not a fixed path.
        // so we need to copy the every thing to the new package path.
        tracing::info!(
            "move source dir {} to package path {}",
            exported_package_path,
            package_path
        );
        let move_script = r#"
mkdir -p -- "$PACKAGE_PATH"
if [ -d "$SOURCE_DIR" ]; then
    find "$SOURCE_DIR" -mindepth 1 -maxdepth 1 -exec mv -- '{}' "$PACKAGE_PATH" \;
fi
        "#;
        let envs = HashMap::from([
            ("PACKAGE_PATH".to_string(), package_path.to_string()),
            ("SOURCE_DIR".to_string(), exported_package_path.clone()),
        ]);
        for layer in package.layers() {
            run_script_unmerge(&[layer], &[], &None, move_script, &envs, &None)?;
        }
    }

    package.validate()?;

    add_import_package(&package)?;

    Ok(())
}

fn migrate_package_store_to_registry(package: &PackageLayer) -> Result<()> {
    crate::registry_layer_store::add_import_registry_package(package)?;
    remove_package_from_store(&package.package_path)?;
    Ok(())
}

pub fn move_package_layer(package_path: &str) -> Result<()> {
    let package = get_package_layer(package_path)?
        .ok_or_else(|| format!("package layer not found in package store: {package_path}"))?;

    let package_name = package
        .name
        .as_deref()
        .ok_or("Failed to move package layer: missing package name")?;
    let version = package
        .version
        .as_deref()
        .ok_or("Failed to move package layer: missing package version")?;

    tracing::info!("move package layer to external store: {package_name}@{version}");

    for layer_name in package.layers() {
        move_layer(&layer_name)?;
    }

    migrate_package_store_to_registry(&package)
}

fn diff(a: HashSet<String>, b: HashSet<String>) -> Vec<String> {
    let diff: Vec<_> = a.difference(&b).collect();
    diff.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Mutex};
    use utils::config::GLOBAL_CONFIG;

    static TEST_CONFIG_LOCK: Mutex<()> = Mutex::new(());

    struct TestStoreGuard {
        original_store_dir: String,
        original_registry_store_file: String,
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    impl TestStoreGuard {
        fn new(base_dir: &Path) -> Self {
            let serial = TEST_CONFIG_LOCK.lock().unwrap();
            let mut config = GLOBAL_CONFIG.lock().unwrap();
            let original_store_dir = config.global.store_dir.clone();
            let original_registry_store_file = config.global.registry_store_file.clone();

            config.global.store_dir = base_dir.join("store").to_string_lossy().into_owned();
            config.global.registry_store_file = base_dir
                .join("registry")
                .join("package_store.json")
                .to_string_lossy()
                .into_owned();
            drop(config);

            Self {
                original_store_dir,
                original_registry_store_file,
                _serial: serial,
            }
        }
    }

    impl Drop for TestStoreGuard {
        fn drop(&mut self) {
            if let Ok(mut config) = GLOBAL_CONFIG.lock() {
                config.global.store_dir = self.original_store_dir.clone();
                config.global.registry_store_file = self.original_registry_store_file.clone();
            }
        }
    }

    struct PathGuard {
        original_path: Option<String>,
    }

    impl PathGuard {
        fn prepend(path: &Path) -> Self {
            let original_path = std::env::var("PATH").ok();
            let mut new_path = path.to_string_lossy().to_string();
            if let Some(existing) = &original_path {
                new_path.push(':');
                new_path.push_str(existing);
            }
            std::env::set_var("PATH", new_path);
            Self { original_path }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            match &self.original_path {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }

    #[test]
    fn test_diff() {
        let a = ["a", "b", "c"];
        let b = ["a", "b", "d"];
        let a: std::collections::HashSet<String> = a.iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = b.iter().map(|s| s.to_string()).collect();
        let diff = super::diff(a, b);
        assert_eq!(diff, vec!["c"]);

        let a = ["a", "b", "c"];
        let b = ["a", "b", "c"];
        let a: std::collections::HashSet<String> = a.iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = b.iter().map(|s| s.to_string()).collect();
        let diff = super::diff(a, b);
        assert_eq!(diff.len(), 0);

        let a = ["a", "b", "c"];
        let b = ["a", "b", "c", "d"];
        let a: std::collections::HashSet<String> = a.iter().map(|s| s.to_string()).collect();
        let b: std::collections::HashSet<String> = b.iter().map(|s| s.to_string()).collect();
        let diff = super::diff(a, b);
        assert_eq!(diff.len(), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_import_package_layer_moves_bootstrap_and_hidden_files() {
        use std::{collections::HashMap, fs};

        use super::*;

        let temp_root = std::env::temp_dir().join(crate::layer::random_name("package_layer_test"));
        let _guard = TestStoreGuard::new(&temp_root);
        let source_dir = temp_root.join("source package");
        let export_dir = temp_root.join("export bundle");
        let imported_path = temp_root.join("imported package");
        let source_dir_str = source_dir.to_string_lossy().to_string();
        let export_dir_str = export_dir.to_string_lossy().to_string();
        let imported_path_str = imported_path.to_string_lossy().to_string();

        fs::create_dir_all(&source_dir).unwrap();
        fs::write(
            source_dir.join("package.oo.yaml"),
            "version: 0.1.2\nscripts:\n  bootstrap: |\n    touch 1.txt\n",
        )
        .unwrap();
        fs::write(source_dir.join(".hidden"), "hidden").unwrap();

        crate::delete_all_layer_data().unwrap();

        let base_layer = create_random_layer().unwrap();
        let package = PackageLayer::create(
            Some("0.1.2".to_string()),
            Some(vec![base_layer]),
            Some("touch 1.txt".to_string()),
            &[],
            source_dir_str.clone(),
            &HashMap::new(),
            &None,
        )
        .unwrap();
        package.export(&export_dir_str).unwrap();

        crate::delete_all_layer_data().unwrap();

        import_package_layer(&imported_path_str, &export_dir_str).unwrap();

        let imported_package = crate::package_store::list_package_layers()
            .unwrap()
            .into_iter()
            .find(|package| package.package_path == imported_path)
            .unwrap();
        let envs = HashMap::from([("PACKAGE_PATH".to_string(), imported_path_str)]);
        run_script_unmerge(
            &imported_package.layers(),
            &[],
            &None,
            r#"
[ -f "$PACKAGE_PATH/package.oo.yaml" ]
[ -f "$PACKAGE_PATH/1.txt" ]
[ -f "$PACKAGE_PATH/.hidden" ]
"#,
            &envs,
            &None,
        )
        .unwrap();

        crate::delete_all_layer_data().unwrap();
        fs::remove_dir_all(temp_root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_migrate_package_store_to_registry_updates_both_stores() {
        use std::{fs, path::PathBuf};

        use super::*;

        let temp_root =
            std::env::temp_dir().join(crate::layer::random_name("package_layer_registry_test"));
        let _guard = TestStoreGuard::new(&temp_root);
        let package_name = "@oomol/import-registry-test";
        let package_version = "0.1.3";
        let package_path = PathBuf::from("/tmp/import-registry-test");
        let package = PackageLayer {
            name: Some(package_name.to_string()),
            version: Some(package_version.to_string()),
            base_layers: Some(vec!["layer_base".to_string()]),
            source_layer: "layer_source".to_string(),
            bootstrap: None,
            bootstrap_layer: Some("layer_bootstrap".to_string()),
            package_path: package_path.clone(),
        };

        add_import_package(&package).unwrap();

        migrate_package_store_to_registry(&package).unwrap();

        let package_store = crate::package_store::load_package_store().unwrap();
        assert!(
            !package_store
                .packages
                .contains_key(&package_path.to_string_lossy().to_string())
        );

        let registry_store = crate::registry_layer_store::load_registry_store().unwrap();
        let key = crate::registry_layer_store::registry_key(package_name, package_version);
        let migrated = registry_store
            .packages
            .get(&key)
            .expect("registry package should exist after migration");
        assert_eq!(migrated.name.as_deref(), Some(package_name));
        assert_eq!(migrated.version.as_deref(), Some(package_version));

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_move_package_layer_moves_layers_and_updates_stores() {
        use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

        use super::*;

        let temp_root = std::env::temp_dir().join(crate::layer::random_name("package_layer_move"));
        let _guard = TestStoreGuard::new(&temp_root);
        let bin_dir = temp_root.join("bin");
        let log_file = temp_root.join("ovmlayer-move.log");
        let package_name = "@oomol/move-package-test";
        let package_version = "0.2.0";
        let package_path = PathBuf::from("/tmp/move-package-test");
        let package = PackageLayer {
            name: Some(package_name.to_string()),
            version: Some(package_version.to_string()),
            base_layers: Some(vec!["layer_base".to_string(), "layer_dep".to_string()]),
            source_layer: "layer_source".to_string(),
            bootstrap: None,
            bootstrap_layer: Some("layer_bootstrap".to_string()),
            package_path: package_path.clone(),
        };

        fs::create_dir_all(&bin_dir).unwrap();
        let fake_ovmlayer = bin_dir.join("ovmlayer");
        fs::write(
            &fake_ovmlayer,
            format!(
                "#!/bin/sh\nset -eu\nif [ \"$1\" = \"move\" ]; then\n  printf '%s\\n' \"$2\" >> \"{}\"\n  exit 0\nfi\nprintf 'unexpected args: %s\\n' \"$*\" >&2\nexit 1\n",
                log_file.display()
            ),
        )
        .unwrap();
        let mut perms = fs::metadata(&fake_ovmlayer).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_ovmlayer, perms).unwrap();
        let _path_guard = PathGuard::prepend(&bin_dir);

        add_import_package(&package).unwrap();

        move_package_layer(package_path.to_str().unwrap()).unwrap();

        let moved_layers = fs::read_to_string(&log_file).unwrap();
        assert_eq!(
            moved_layers.lines().collect::<Vec<_>>(),
            package
                .layers()
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
        );

        let package_store = crate::package_store::load_package_store().unwrap();
        assert!(
            !package_store
                .packages
                .contains_key(&package_path.to_string_lossy().to_string())
        );

        let registry_store = crate::registry_layer_store::load_registry_store().unwrap();
        let key = crate::registry_layer_store::registry_key(package_name, package_version);
        let moved_package = registry_store
            .packages
            .get(&key)
            .expect("registry package should exist after move");
        assert_eq!(moved_package.package_path, package_path);
        assert_eq!(moved_package.layers(), package.layers());

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a writable ovmlayer workspace configured by `ovmlayer setup`"]
    fn test_move_package_layer_with_real_ovmlayer_and_base_layers() {
        use std::{collections::HashMap, fs};

        use super::*;

        let temp_root =
            std::env::temp_dir().join(crate::layer::random_name("package_layer_move_real"));
        let _guard = TestStoreGuard::new(&temp_root);
        let source_dir = temp_root.join("source_package");
        let source_dir_str = source_dir.to_string_lossy().to_string();
        let package_name = "@oomol/move-real-base-test";
        let package_version = "0.4.0";

        fs::create_dir_all(&source_dir).unwrap();
        fs::write(
            source_dir.join("package.oo.yaml"),
            format!(
                "name: \"{package_name}\"\nversion: {package_version}\nscripts:\n  bootstrap: |\n    touch moved.txt\n"
            ),
        )
        .unwrap();

        let base_layer = create_random_layer().expect("create base layer failed");
        let package = PackageLayer::create(
            Some(package_version.to_string()),
            Some(vec![base_layer.clone()]),
            Some("touch moved.txt".to_string()),
            &[],
            source_dir_str,
            &HashMap::new(),
            &None,
        )
        .expect("create package layer failed");

        add_import_package(&package).unwrap();

        move_package_layer(package.package_path.to_str().unwrap())
            .expect("move package layer failed");

        let registry_package =
            crate::registry_layer_store::get_registry_layer(package_name, package_version)
                .unwrap()
                .expect("registry package should exist after move");
        assert_eq!(registry_package.package_path, package.package_path);
        assert_eq!(registry_package.base_layers, Some(vec![base_layer]));
        assert_eq!(registry_package.source_layer, package.source_layer);
        assert_eq!(registry_package.bootstrap_layer, package.bootstrap_layer);

        let package_status =
            crate::package_store::package_layer_status(&package.package_path).unwrap();
        assert_eq!(
            package_status,
            crate::package_store::PackageLayerStatus::NotInStore
        );

        fs::remove_dir_all(temp_root).unwrap();
    }
}
