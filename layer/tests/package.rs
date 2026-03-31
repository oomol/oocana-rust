#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {

    use ctor::ctor;
    use layer::*;
    use std::{collections::HashMap, path::PathBuf, sync::Mutex};
    use tracing::{self, info};
    use tracing_subscriber;
    use utils::config::GLOBAL_CONFIG;

    static TEST_CONFIG_LOCK: Mutex<()> = Mutex::new(());

    struct TestStoreGuard {
        original_store_dir: String,
        original_registry_store_file: String,
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    impl TestStoreGuard {
        fn new(base_dir: &std::path::Path) -> Self {
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

    fn dirname() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
    }

    #[ctor]
    fn init() {
        init_tracing();
    }

    fn init_tracing() {
        let subscriber = tracing_subscriber::fmt::Subscriber::builder()
            .with_max_level(tracing::Level::TRACE)
            .with_file(true)
            .with_line_number(true)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("setting default subscriber failed");
    }

    #[test]
    fn test_package_layer_api() {
        let d = dirname().join("data").join("vim");
        let r = get_or_create_package_layer(&d, &vec![], &HashMap::new(), &None);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());
        info!("get_package_layer: {:?}", r.unwrap());

        let r = package_layer_status(&d);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());
        info!("package_layer_status: {:?}", r.unwrap());

        let r = list_package_layers();
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());
        for l in r.unwrap() {
            info!("layer: {:#?}", l);
        }

        let r = delete_package_layer(&d);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());
    }

    #[test]
    fn test_package_layer_store() {
        let d = dirname().join("data").join("simple");
        let r = get_or_create_package_layer(&d, &vec![], &HashMap::new(), &None);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());

        let r = package_layer_status(&d);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());

        let status = r.unwrap();
        assert_eq!(status, PackageLayerStatus::Exist);
    }

    #[test]
    fn test_validate_package() {
        let d = dirname().join("data").join("simple");
        let r = get_or_create_package_layer(&d, &vec![], &HashMap::new(), &None);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());
        let package_layer = r.unwrap();
        let result = package_layer.validate();
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());

        let result = delete_package_layer(d);
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());

        let result = package_layer.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_export_import() {
        let d = dirname().join("data").join("simple");
        let layer = get_or_create_package_layer(&d, &vec![], &HashMap::new(), &None);
        assert!(layer.is_ok(), "Error: {:?}", layer.unwrap_err());
        let package_layer = layer.unwrap();

        let export_dir = "/tmp/simple";

        let result = package_layer.export(export_dir);
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());

        delete_all_layer_data().unwrap();

        let new_package = "/a/b/c/d";

        let result = import_package_layer(new_package, "/tmp/layer-not-exist");
        assert!(result.is_err(), "Error: {:?}", result);

        let result = import_package_layer(new_package, export_dir);
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());
    }

    #[test]
    #[ignore = "requires a writable ovmlayer workspace configured by `ovmlayer setup`"]
    fn test_move_package_layer_with_real_ovmlayer() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("move-package-layer-real-{}", suffix));
        let _guard = TestStoreGuard::new(&temp_root);

        let probe_name = format!("probe_layer_{suffix}");
        let probe = std::process::Command::new("ovmlayer")
            .args(["create", &probe_name])
            .output();
        assert!(
            probe.is_ok(),
            "ovmlayer workspace is not writable or not fully configured: {:?}",
            probe.err()
        );
        let probe = probe.unwrap();
        assert!(
            probe.status.success(),
            "ovmlayer workspace is not writable or not fully configured: {:?}",
            probe
        );
        let cleanup = std::process::Command::new("ovmlayer")
            .args(["delete", &probe_name])
            .output()
            .unwrap();
        assert!(
            cleanup.status.success(),
            "failed to cleanup probe layer: {:?}",
            cleanup
        );

        let package_dir = temp_root.join("pkg");
        std::fs::create_dir_all(&package_dir).unwrap();
        std::fs::write(
            package_dir.join("package.oo.yaml"),
            r#"name: "@oomol/move-real-test"
version: 0.3.0
scripts:
  bootstrap: |
    touch moved.txt
"#,
        )
        .unwrap();

        let package = get_or_create_package_layer(&package_dir, &vec![], &HashMap::new(), &None)
            .expect("create package layer failed");
        assert_eq!(
            package_layer_status(&package_dir).unwrap(),
            PackageLayerStatus::Exist
        );

        move_package_layer(package_dir.to_str().unwrap()).expect("move package layer failed");

        assert_eq!(
            package_layer_status(&package_dir).unwrap(),
            PackageLayerStatus::NotInStore
        );

        let registry_package = get_registry_layer("@oomol/move-real-test", "0.3.0")
            .unwrap()
            .expect("registry package should exist after move");
        assert_eq!(registry_package.package_path, package_dir);
        assert_eq!(registry_package.layers(), package.layers());

        std::fs::remove_dir_all(temp_root).unwrap();
    }
}
