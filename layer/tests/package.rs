#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {

    use ctor::ctor;
    use layer::*;
    use std::path::PathBuf;
    use tracing::{self, info};
    use tracing_subscriber;

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
        let r = get_or_create_package_layer(&d, &vec![]);
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
        let r = get_or_create_package_layer(&d, &vec![]);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());

        let r = package_layer_status(&d);
        assert!(r.is_ok(), "Error: {:?}", r.unwrap_err());

        let status = r.unwrap();
        assert_eq!(status, PackageLayerStatus::Exist);
    }

    #[test]
    fn test_validate_package() {
        let d = dirname().join("data").join("simple");
        let r = get_or_create_package_layer(&d, &vec![]);
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
        let layer = get_or_create_package_layer(&d, &vec![]);
        assert!(layer.is_ok(), "Error: {:?}", layer.unwrap_err());
        let package_layer = layer.unwrap();

        let result = package_layer.export("/tmp/layer");
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());

        delete_all_layer_data().unwrap();

        let result = import_package_layer("/tmp/layer", "/tmp/layer-not-exist");
        assert!(result.is_err(), "Error: {:?}", result);

        let result = import_package_layer("/tmp/layer", "/tmp/layer");
        assert!(result.is_ok(), "Error: {:?}", result.unwrap_err());
    }
}
