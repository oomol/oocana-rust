use std::path::{Path, PathBuf};

use config::Config;
use lazy_static::lazy_static;
use std::sync::Mutex;

use crate::path::expand_home;
use dirs::home_dir;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TmpGlobalConfig {
    pub store_dir: Option<String>,
    pub oocana_dir: Option<String>,
    pub env_file: Option<String>,
    pub bind_path_file: Option<String>,
}

impl From<TmpGlobalConfig> for GlobalConfig {
    fn from(tmp: TmpGlobalConfig) -> Self {
        let store_dir = tmp.store_dir.map(|s| expand_home(&s));
        let oocana_dir = tmp.oocana_dir.map(|s| expand_home(&s));
        let env_file = tmp.env_file.map(|s| expand_home(&s));
        let bind_path_file = tmp.bind_path_file.map(|s| expand_home(&s));

        GlobalConfig {
            store_dir: store_dir,
            oocana_dir: oocana_dir,
            env_file: env_file,
            bind_path_file: bind_path_file,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(from = "TmpGlobalConfig")]
pub struct GlobalConfig {
    pub store_dir: Option<String>,
    pub oocana_dir: Option<String>,
    pub env_file: Option<String>,
    pub bind_path_file: Option<String>,
}

struct TmpRunExtraConfig {
    pub search_paths: Option<Vec<String>>,
}

impl From<TmpRunExtraConfig> for RunExtraConfig {
    fn from(tmp: TmpRunExtraConfig) -> Self {
        RunExtraConfig {
            search_paths: tmp.search_paths.map(|paths| {
                paths
                    .into_iter()
                    .map(|s| expand_home(&s))
                    .collect::<Vec<String>>()
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunExtraConfig {
    pub search_paths: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(from = "TmpRunConfig")]
struct TmpRunConfig {
    pub search_paths: Option<Vec<String>>,
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}

impl From<TmpRunConfig> for RunConfig {
    fn from(tmp: TmpRunConfig) -> Self {
        let extra = tmp.extra.map(|e| RunExtraConfig {
            search_paths: e.search_paths,
        });

        let search_paths = tmp.search_paths.map(|s| {
            s.into_iter()
                .map(|s| expand_home(&s))
                .collect::<Vec<String>>()
        });
        let exclude_packages = tmp.exclude_packages.map(|s| {
            s.into_iter()
                .map(|s| expand_home(&s))
                .collect::<Vec<String>>()
        });

        RunConfig {
            search_paths: search_paths,
            exclude_packages: exclude_packages,
            reporter: tmp.reporter,
            debug: tmp.debug,
            extra: extra,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunConfig {
    pub search_paths: Option<Vec<String>>,
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub global: Option<GlobalConfig>,
    pub run: Option<RunConfig>,
}

lazy_static! {
    pub static ref GLOBAL_CONFIG: Mutex<AppConfig> = Mutex::new(AppConfig {
        global: None,
        run: None,
    });
}

pub fn load_config<P: AsRef<Path>>(file: Option<P>) -> Result<AppConfig, String> {
    let p: PathBuf = match file {
        Some(p) => p.as_ref().to_path_buf(),
        None => {
            let mut default_path = home_dir().ok_or("Failed to get home dir")?;
            default_path.push("oocana");
            default_path.push("config");
            default_path.to_path_buf()
        }
    };

    let path = if p.starts_with("~") || p.starts_with("$HOME") {
        let home = home_dir().ok_or_else(|| "Failed to get home dir".to_string())?;
        let stripped_path = if p.starts_with("~") {
            p.strip_prefix("~").unwrap()
        } else {
            p.strip_prefix("$HOME").unwrap()
        };
        let mut home_path = home;
        home_path.push(stripped_path);
        home_path
    } else if p.is_absolute() {
        p.to_path_buf()
    } else if p.is_relative() {
        let mut current_dir =
            std::env::current_dir().map_err(|e| format!("Failed to get current dir: {:?}", e))?;
        current_dir.push(&p);
        current_dir
    } else {
        p.to_path_buf()
    };

    Config::builder()
        .add_source(config::File::with_name(&path.to_string_lossy()))
        .build()
        .map_err(|e| format!("Failed to load config: {:?}", e))?
        .try_deserialize::<AppConfig>()
        .map_err(|e| format!("Failed to deserialize config: {:?}", e))
        .map(|config| {
            let mut global_config = GLOBAL_CONFIG.lock().unwrap();
            *global_config = config;
            global_config.clone()
        })
}

pub fn store_dir() -> Option<PathBuf> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();

    let store_dir = global_config
        .global
        .as_ref()
        .and_then(|g| g.store_dir.as_ref())
        .map(|s| PathBuf::from(s));

    match store_dir {
        Some(store_dir) => Some(store_dir),
        None => home_dir().map(|mut h| {
            h.push(".oomol-studio");
            h.push("oocana");
            h
        }),
    }
}

pub fn oocana_dir() -> Option<PathBuf> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();

    let oocana_dir = global_config
        .global
        .as_ref()
        .and_then(|g| g.oocana_dir.as_ref())
        .map(|s| PathBuf::from(s));

    match oocana_dir {
        Some(oocana_dir) => Some(oocana_dir),
        None => home_dir().map(|mut h| {
            h.push(".oocana");
            h
        }),
    }
}
