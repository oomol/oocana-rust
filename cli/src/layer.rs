use std::collections::HashMap;

use clap::Subcommand;
use layer::{import_package_layer, BindPath};
use manifest_reader::path_finder::find_package_file;
use tracing::info;
use utils::error::{Error, Result};

#[derive(Debug, Subcommand)]
pub enum LayerAction {
    #[command(about = "create package layer")]
    Create {
        #[arg(help = "package path")]
        package: String,
        #[arg(
            help = "some bind paths, format <source_path>:<target_path>, accept multiple input. example: --bind-paths <source>:<target> --bind-paths <source>:<target>",
            long
        )]
        bind_paths: Option<Vec<String>>,
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
    },
    Scan {
        #[arg(help = "package search path dir which sub directory has package", long)]
        search_paths: Vec<String>,
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
        dir: String,
        #[arg(help = "package path")]
        package: String,
    },
    #[command(about = "list package layer")]
    List {},
    #[command(about = "delete all package layer")]
    DeleteAll {},
}

pub fn layer_action(action: &LayerAction) -> Result<()> {
    if std::env::var(layer::OVMLAYER_LOG_ENV_KEY).is_err() {
        std::env::set_var(
            layer::OVMLAYER_LOG_ENV_KEY,
            utils::logger::logger_dir()
                .join("ovmlayer.log")
                .to_string_lossy()
                .to_string(),
        );
    }

    match action {
        LayerAction::Create {
            package,
            bind_paths,
        } => {
            let mut bind_path_arg: Vec<BindPath> = vec![];
            if let Some(paths) = bind_paths {
                for path in paths {
                    let parts = path.split(':').collect::<Vec<&str>>();
                    if parts.len() == 2 {
                        bind_path_arg.push(BindPath {
                            source: parts[0].to_string(),
                            target: parts[1].to_string(),
                        });
                    }
                }
            }
            layer::get_or_create_package_layer(package, &bind_path_arg)?;
        }
        LayerAction::Delete { package } => {
            layer::delete_package_layer(package)?;
        }
        LayerAction::Get { package } => {
            let status = layer::package_layer_status(package)?;
            info!("package ({package}) status: {status:?}");
            println!("{status:?}");
        }
        LayerAction::Scan { search_paths } => {
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

                        if find_package_file(&path).is_some() {
                            tracing::debug!("find package file in {path:?}");
                            let status = layer::package_layer_status(&path)?;
                            package_map.insert(path, format!("{status:?}"));
                        }
                    }
                } else {
                    tracing::warn!("directory {p:?} is not a directory");
                }
            }
            println!("{package_map:?}");
        }
        LayerAction::Export { package, dest } => {
            let status = layer::package_layer_status(package)?;
            match status {
                layer::PackageLayerStatus::Exist => {
                    let l = layer::get_or_create_package_layer(package, &vec![])?;
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
        LayerAction::Import { package, dir } => {
            let status = layer::package_layer_status(package)?;
            match status {
                layer::PackageLayerStatus::Exist => {
                    return Err(Error::from(format!(
                        "Package layer {:?} already exists",
                        package
                    )));
                }
                layer::PackageLayerStatus::NotInStore => import_package_layer(package, dir)?,
            }
        }
        LayerAction::List {} => {
            let r = layer::list_package_layers()?;
            for l in r {
                let info = format!("Package: {:?}, Version: {:?}", l.package_path, l.version);
                println!("{info}");
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
