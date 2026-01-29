use std::collections::HashMap;

use crate::fun::arg::{find_env_file, load_bind_paths};
use clap::Subcommand;
use manifest_reader::path_finder::find_package_file;
use manifest_reader::reader::read_package;
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
    #[command(about = "create registry package layer")]
    CreateRegistry {
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
    #[command(about = "get package layer")]
    Get {
        #[arg(help = "package path")]
        package: String,
        #[arg(help = "package name for registry layer (optional)", long)]
        package_name: Option<String>,
        #[arg(help = "package version for registry layer (optional)", long)]
        version: Option<String>,
    },
    #[command(
        about = "get registry package layer (deprecated: use 'get' with --package-name and --version)"
    )]
    GetRegistry {
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
        layer_dir: String,
        #[arg(help = "package path")]
        import_package: String,
    },
    #[command(about = "list package layer")]
    List {},
    #[command(about = "delete all package layer")]
    DeleteAll {},
}

/// Get layer status with registry fallback logic (same as Get command).
/// If override_name and override_version are provided, they take precedence over package meta.
/// Returns Exist if registry or package layer exists, NotInStore otherwise.
/// Errors are logged but treated as NotInStore.
fn get_layer_status_with_registry_fallback(
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

    // If we have both name and version, try registry layer first
    if let (Some(ref name), Some(ref ver)) = (&pkg_name, &ver) {
        match layer::registry_layer_status(name, ver) {
            Ok(layer::RegistryLayerStatus::Exist) => {
                tracing::debug!(
                    "find registry layer ({name}@{ver}) in {package_path:?} with status Exist"
                );
                return layer::PackageLayerStatus::Exist;
            }
            Ok(layer::RegistryLayerStatus::NotInStore) => {
                tracing::debug!(
                    "registry layer ({name}@{ver}) not in store, fallback to package layer"
                );
            }
            Err(e) => {
                tracing::debug!(
                    "get registry layer ({name}@{ver}) failed: {:?}, fallback to package layer",
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

pub fn layer_action(action: &LayerAction) -> Result<()> {
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
        LayerAction::CreateRegistry {
            package_name,
            version,
            package,
            bind_paths,
            bind_path_file,
            retain_env_keys,
            env_file,
        } => {
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

            layer::create_registry_layer(
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
        LayerAction::Get {
            package,
            package_name,
            version,
        } => {
            let status = get_layer_status_with_registry_fallback(
                std::path::Path::new(package),
                package_name.as_deref(),
                version.as_deref(),
            );

            info!("package ({package}) status: {status:?}");
            println!("{status:?}");
        }
        LayerAction::GetRegistry {
            package_name,
            version,
        } => {
            let status = layer::registry_layer_status(package_name, version)?;
            info!("registry package ({package_name}@{version}) status: {status:?}");
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
                                    let status =
                                        get_layer_status_with_registry_fallback(&scope_path, None, None);
                                    package_map.insert(scope_path, format!("{status:?}"));
                                }
                            }
                        } else {
                            // Regular directories, use original logic
                            if find_package_file(&path).is_some() {
                                let status =
                                    get_layer_status_with_registry_fallback(&path, None, None);
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
                        "Package layer {:?} doesn't exist",
                        package
                    )));
                }
            }
        }
        LayerAction::Import {
            import_package,
            layer_dir,
        } => {
            let status = layer::package_layer_status(import_package);
            match status {
                Ok(layer::PackageLayerStatus::NotInStore) => {
                    layer::import_package_layer(import_package, layer_dir)?;
                }
                Ok(layer::PackageLayerStatus::Exist) => {
                    info!("Package layer {:?} already exists, skip import", import_package);
                    println!("Package layer already exists, skipping import.");
                }
                Err(e) => {
                    tracing::info!("import package path doesn't exist package file: {:?}. just import package layer.", e);
                    layer::import_package_layer(import_package, layer_dir)?;
                }
            }
        }
        LayerAction::List {} => {
            let r = layer::list_package_layers()?;
            for l in r {
                let info = format!("Package: {:?}, Version: {:?}", l.package_path, l.version);
                println!("{info}");
            }

            match layer::load_registry_store() {
                Ok(store) => {
                    for (key, l) in store.packages {
                        let info = format!("Registry: {}, Path: {:?}", key, l.package_path);
                        println!("{info}");
                    }
                }
                Err(e) => {
                    tracing::warn!("load registry store failed: {e}");
                }
            }
        }
        LayerAction::DeleteAll {} => {
            // TODO: 修改成 prune API，不要清理所有的 layer data，会有误删的问题
            tracing::warn!("delete all layer data is not recommended, it will delete all layer data, including the layers that are used by other projects");
            layer::delete_all_layer_data()?;
        }
    }

    Ok(())
}
