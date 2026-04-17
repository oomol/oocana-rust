use std::collections::HashMap;
use std::path::Path;

use crate::fun::arg::{find_env_file, load_bind_paths};
use clap::Subcommand;
use manifest_reader::path_finder::find_package_file;
use manifest_reader::reader::{
    is_connector_package_name, read_block_metadata, read_package, read_package_identity,
    should_skip_package_layer_handling, should_skip_package_layer_handling_for_path,
};
use std::io::Write;
use tracing::info;
use utils::error::{Error, Result};

#[derive(Debug, Subcommand)]
pub enum LayerAction {
    #[command(about = "create package layer")]
    Create {
        #[arg(help = "package path")]
        package: String,
        #[arg(
            help = "bind paths, format src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive (rw,nonrecursive is default value), accept multiple input. example: --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive",
            long
        )]
        bind_paths: Option<Vec<String>>,
        #[arg(
            help = "a file path contains multiple bind paths. The file format is src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive (rw,nonrecursive is default value) line by line, if not provided, it will be found in OOCANA_BIND_PATH_FILE env variable",
            long
        )]
        bind_path_file: Option<String>,
        #[arg(
            help = "pass the environment variables(only accept variable name) to layer creation. accept multiple input. example: --retain-env-keys <env> --retain-env-keys <env>",
            long
        )]
        retain_env_keys: Option<Vec<String>>,
        #[arg(
            help = ".env file path, when create a layer, these env will pass to this process. The file format is <key>=<value> line by line like traditional env file. if not provided, oocana will search OOCANA_ENV_FILE env variable",
            long
        )]
        env_file: Option<String>,
    },
    #[command(about = "create external package layer")]
    CreateExternal {
        #[arg(help = "package name")]
        package_name: String,
        #[arg(help = "package version")]
        version: String,
        #[arg(help = "package path")]
        package: String,
        #[arg(
            help = "bind paths, format src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive (rw,nonrecursive is default value), accept multiple input. example: --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive",
            long
        )]
        bind_paths: Option<Vec<String>>,
        #[arg(
            help = "a file path contains multiple bind paths. The file format is src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive (rw,nonrecursive is default value) line by line, if not provided, it will be found in OOCANA_BIND_PATH_FILE env variable",
            long
        )]
        bind_path_file: Option<String>,
        #[arg(
            help = "pass the environment variables(only accept variable name) to layer creation. accept multiple input. example: --retain-env-keys <env> --retain-env-keys <env>",
            long
        )]
        retain_env_keys: Option<Vec<String>>,
        #[arg(
            help = ".env file path, when create a layer, these env will pass to this process. The file format is <key>=<value> line by line like traditional env file. if not provided, oocana will search OOCANA_ENV_FILE env variable",
            long
        )]
        env_file: Option<String>,
    },
    #[command(about = "delete package layer")]
    Delete {
        #[arg(help = "package path")]
        package: String,
    },
    #[command(about = "delete external package layer")]
    DeleteExternal {
        #[arg(help = "package name")]
        package_name: String,
        #[arg(help = "package version")]
        version: String,
    },
    #[command(about = "get package layer")]
    Get {
        #[arg(help = "package path")]
        package: String,
        #[arg(help = "package name for external layer (optional)", long)]
        package_name: Option<String>,
        #[arg(help = "package version for external layer (optional)", long)]
        version: Option<String>,
    },
    #[command(
        about = "get external package layer (deprecated: use 'get' with --package-name and --version)"
    )]
    GetExternal {
        #[arg(help = "package name")]
        package_name: String,
        #[arg(help = "package version")]
        version: String,
    },
    Scan {
        #[arg(help = "package search path dir which sub directory has package", long)]
        search_paths: Vec<String>,
        #[arg(
            help = "output file path (JSON format), if not provided, it will print to stdout",
            long
        )]
        output: Option<String>,
    },
    #[command(about = "export package layer. It requires the package layer exists")]
    Export {
        #[arg(help = "package path")]
        package: String,
        #[arg(help = "export path")]
        dest: String,
    },
    #[command(about = "import package layer. It requires the package layer doesn't exist")]
    Import {
        #[arg(help = "directory that contains package layer's export files")]
        export_dir: String,
        #[arg(
            help = "which package path to import, it should be the same as the package path in the exported layer"
        )]
        package_path: String,
    },
    #[command(about = "move package layer from package store to ovmlayer external store")]
    Mv {
        #[arg(help = "package path")]
        package: String,
    },
    #[command(about = "list package layer")]
    List {},
    #[command(about = "delete all package layer")]
    DeleteAll {},
}

/// Get layer status, checking external layer store first and falling back to package layer store.
/// If override_name and override_version are provided, they take precedence over package meta.
/// Returns Exist if external or package layer exists, NotInStore otherwise.
/// Errors are logged but treated as NotInStore.
fn get_layer_status_external_first(
    package_path: &std::path::Path,
    override_name: Option<&str>,
    override_version: Option<&str>,
) -> layer::PackageLayerStatus {
    // Use overrides if both provided, otherwise try to read from package meta
    let (pkg_name, ver) = match (override_name, override_version) {
        (Some(name), Some(ver)) => (Some(name.to_string()), Some(ver.to_string())),
        _ => find_package_file(package_path)
            .and_then(|pkg_file| read_package(&pkg_file).ok())
            .map(|meta| (meta.name, meta.version))
            .unwrap_or((None, None)),
    };

    // If we have both name and version, try external layer first
    if let (Some(ref name), Some(ref ver)) = (&pkg_name, &ver) {
        match layer::external_layer_status(name, ver) {
            Ok(layer::ExternalLayerStatus::Exist) => {
                tracing::debug!(
                    "find external layer ({name}@{ver}) in {package_path:?} with status Exist"
                );
                return layer::PackageLayerStatus::Exist;
            }
            Ok(layer::ExternalLayerStatus::NotInStore) => {
                tracing::debug!(
                    "external layer ({name}@{ver}) not in store, fallback to package layer"
                );
            }
            Err(e) => {
                tracing::debug!(
                    "get external layer ({name}@{ver}) failed: {:?}, fallback to package layer",
                    e
                );
            }
        }
    }

    // Fallback to package layer
    match layer::package_layer_status(package_path) {
        Ok(status) => {
            tracing::debug!("package {package_path:?} status: {status:?}");
            status
        }
        Err(e) => {
            tracing::warn!("get package layer status failed for {package_path:?}: {e}");
            layer::PackageLayerStatus::NotInStore
        }
    }
}

fn package_layer_skip_reason(package_path: &Path) -> Option<&'static str> {
    let metadata = read_block_metadata(package_path);
    if metadata.hide_source {
        return Some("hide_source is enabled");
    }

    let package_name = read_package_identity(package_path).and_then(|identity| identity.name);
    if should_skip_package_layer_handling(&metadata, package_name.as_deref())
        && is_connector_package_name(package_name.as_deref())
    {
        return Some("package name starts with @connector");
    }

    None
}

fn package_layer_status_for_query(
    package_path: &Path,
    override_name: Option<&str>,
    override_version: Option<&str>,
) -> layer::PackageLayerStatus {
    if should_skip_package_layer_handling_for_path(package_path) {
        return layer::PackageLayerStatus::Exist;
    }

    get_layer_status_external_first(package_path, override_name, override_version)
}

pub fn layer_action(action: &LayerAction) -> Result<()> {
    if std::env::var(utils::env::OVMLAYER_LOG_ENV_KEY).is_err() {
        std::env::set_var(
            utils::env::OVMLAYER_LOG_ENV_KEY,
            utils::logger::logger_dir()
                .join("ovmlayer.log")
                .to_string_lossy()
                .to_string(),
        );
    }

    if !layer::feature_enabled() {
        tracing::warn!("Layer feature is not enabled. quitting");
        return Err(Error::from("Layer feature is not enabled"));
    }

    match action {
        LayerAction::Create {
            package,
            bind_paths,
            bind_path_file,
            retain_env_keys,
            env_file,
        } => {
            if let Some(reason) = package_layer_skip_reason(std::path::Path::new(package)) {
                return Err(Error::from(format!(
                    "Cannot create layer for package {package}: {reason}"
                )));
            }
            let bind_path_arg = load_bind_paths(bind_paths, bind_path_file);
            let envs: HashMap<String, String> = std::env::vars()
                .filter(|(key, _)| {
                    key.starts_with("OOMOL_")
                        || retain_env_keys
                            .as_ref()
                            .is_some_and(|list| list.contains(key))
                })
                .collect();

            let env_file = find_env_file(env_file);

            layer::get_or_create_package_layer(package, &bind_path_arg, &envs, &env_file)?;
        }
        LayerAction::CreateExternal {
            package_name,
            version,
            package,
            bind_paths,
            bind_path_file,
            retain_env_keys,
            env_file,
        } => {
            if manifest_reader::reader::read_block_metadata(std::path::Path::new(package))
                .hide_source
            {
                return Err(Error::from(format!(
                    "Cannot create external layer for package {package}: hide_source is enabled"
                )));
            }
            let bind_path_arg = load_bind_paths(bind_paths, bind_path_file);
            let envs: HashMap<String, String> = std::env::vars()
                .filter(|(key, _)| {
                    key.starts_with("OOMOL_")
                        || retain_env_keys
                            .as_ref()
                            .is_some_and(|list| list.contains(key))
                })
                .collect();

            let env_file = find_env_file(env_file);

            layer::create_external_layer(
                package_name,
                version,
                package,
                &bind_path_arg,
                &envs,
                &env_file,
            )?;
        }
        LayerAction::Delete { package } => {
            layer::delete_package_layer(package)?;
        }
        LayerAction::DeleteExternal {
            package_name,
            version,
        } => {
            layer::delete_external_layer(package_name, version)?;
        }
        LayerAction::Get {
            package,
            package_name,
            version,
        } => {
            if let Some(reason) = package_layer_skip_reason(std::path::Path::new(package)) {
                info!(
                    "package ({package}) skips package-layer handling ({reason}), reporting Exist"
                );
            }
            let status = package_layer_status_for_query(
                std::path::Path::new(package),
                package_name.as_deref(),
                version.as_deref(),
            );

            info!("package ({package}) status: {status:?}");
            println!("{status:?}");
        }
        LayerAction::GetExternal {
            package_name,
            version,
        } => {
            let status = layer::external_layer_status(package_name, version)?;
            info!("external package ({package_name}@{version}) status: {status:?}");
            println!("{status:?}");
        }
        LayerAction::Scan {
            search_paths,
            output,
        } => {
            let mut package_map = HashMap::new();
            for dir in search_paths {
                let p = std::path::PathBuf::from(dir);
                if p.is_dir() {
                    let entries = std::fs::read_dir(p)?;
                    for entry in entries {
                        let entry = entry?;
                        let path = entry.path();
                        if !path.is_dir() {
                            continue;
                        }

                        // Check if this is a scope directory (starts with @)
                        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                        if dir_name.starts_with('@') {
                            // For @scope directories, scan their subdirectories
                            let scope_entries = std::fs::read_dir(&path)?;
                            for scope_entry in scope_entries {
                                let scope_entry = scope_entry?;
                                let scope_path = scope_entry.path();
                                if !scope_path.is_dir() {
                                    continue;
                                }

                                if find_package_file(&scope_path).is_some() {
                                    if let Some(reason) = package_layer_skip_reason(&scope_path) {
                                        info!(
                                            "package ({}) skips package-layer handling ({}), reporting Exist",
                                            scope_path.display(),
                                            reason
                                        );
                                    }
                                    let status =
                                        package_layer_status_for_query(&scope_path, None, None);
                                    package_map.insert(scope_path, format!("{status:?}"));
                                }
                            }
                        } else {
                            // Regular directories, use original logic
                            if find_package_file(&path).is_some() {
                                if let Some(reason) = package_layer_skip_reason(&path) {
                                    info!(
                                        "package ({}) skips package-layer handling ({}), reporting Exist",
                                        path.display(),
                                        reason
                                    );
                                }
                                let status = package_layer_status_for_query(&path, None, None);
                                package_map.insert(path, format!("{status:?}"));
                            }
                        }
                    }
                } else {
                    tracing::warn!("directory {p:?} is not a directory");
                }
            }

            if let Some(output_path) = output {
                let mut file = std::fs::File::create(output_path)?;
                write!(file, "{package_map:?}")?;
                file.flush()?;
                tracing::info!("scan result written to {output_path}");
            } else {
                tracing::info!("scan result: {package_map:?}");
                println!("{package_map:?}");
            }
        }
        LayerAction::Export { package, dest } => {
            let status = layer::package_layer_status(package)?;
            match status {
                layer::PackageLayerStatus::Exist => {
                    let l =
                        layer::get_or_create_package_layer(package, &[], &HashMap::new(), &None)?;
                    l.export(dest)?;
                }
                layer::PackageLayerStatus::NotInStore => {
                    return Err(Error::from(format!(
                        "Package layer {package:?} doesn't exist"
                    )));
                }
            }
        }
        LayerAction::Import {
            package_path,
            export_dir,
        } => {
            let status = layer::package_layer_status(package_path);
            match status {
                Ok(layer::PackageLayerStatus::NotInStore) => {
                    layer::import_package_layer(package_path, export_dir)?;
                }
                Ok(status) => {
                    if matches!(status, layer::PackageLayerStatus::Exist) {
                        info!(
                            "Package layer {:?} already exists, skip import",
                            package_path
                        );
                        println!("Package layer already exists, skipping import.");
                    } else {
                        layer::import_package_layer(package_path, export_dir)?;
                    }
                }
                Err(e) => {
                    tracing::info!(
                        "import package path doesn't exist package file: {:?}. just import package layer.",
                        e
                    );
                    layer::import_package_layer(package_path, export_dir)?;
                }
            }
        }
        LayerAction::Mv { package } => {
            layer::move_package_layer(package)?;
        }
        LayerAction::List {} => {
            let r = layer::list_package_layers()?;
            for l in r {
                let info = format!("Package: {:?}, Version: {:?}", l.package_path, l.version);
                println!("{info}");
            }

            match layer::load_external_store() {
                Ok(store) => {
                    for (key, l) in store.packages {
                        let info = format!("External: {}, Path: {:?}", key, l.package_path);
                        println!("{info}");
                    }
                }
                Err(e) => {
                    tracing::warn!("load external store failed: {e}");
                }
            }
        }
        LayerAction::DeleteAll {} => {
            // TODO: 修改成 prune API，不要清理所有的 layer data，会有误删的问题
            tracing::warn!(
                "delete all layer data is not recommended, it will delete all layer data, including the layers that are used by other projects"
            );
            layer::delete_all_layer_data()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn connector_packages_are_rejected_for_package_layer_create() {
        let package = workspace_root().join("tests/fixtures/@connector/demo");

        assert_eq!(
            package_layer_skip_reason(&package),
            Some("package name starts with @connector")
        );
    }

    #[test]
    fn connector_packages_report_exist_for_layer_queries() {
        let package = workspace_root().join("tests/fixtures/@connector/demo");

        assert_eq!(
            package_layer_status_for_query(&package, None, None),
            layer::PackageLayerStatus::Exist
        );
    }

    #[test]
    fn hide_source_packages_still_report_exist_for_layer_queries() {
        let package = workspace_root().join("examples/remote_task/hide-example");

        assert_eq!(
            package_layer_skip_reason(&package),
            Some("hide_source is enabled")
        );
        assert_eq!(
            package_layer_status_for_query(&package, None, None),
            layer::PackageLayerStatus::Exist
        );
    }
}
