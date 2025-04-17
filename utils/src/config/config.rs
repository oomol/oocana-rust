use crate::path::expand_home;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TmpGlobalConfig {
    #[serde(default = "default_store_dir")]
    pub store_dir: String,
    #[serde(default = "default_oocana_dir")]
    pub oocana_dir: String,
    pub env_file: Option<String>,
    pub bind_path_file: Option<String>,
    pub search_paths: Option<Vec<String>>,
}

fn default_store_dir() -> String {
    "~/.oomol-studio/oocana".to_string()
}

fn default_oocana_dir() -> String {
    "~/.oocana".to_string()
}

impl Default for TmpGlobalConfig {
    fn default() -> Self {
        TmpGlobalConfig {
            store_dir: default_store_dir(),
            oocana_dir: default_oocana_dir(),
            env_file: None,
            bind_path_file: None,
            search_paths: None,
        }
    }
}

impl From<TmpGlobalConfig> for GlobalConfig {
    fn from(tmp: TmpGlobalConfig) -> Self {
        let store_dir = expand_home(&tmp.store_dir);
        let oocana_dir = expand_home(&tmp.oocana_dir);
        let env_file = tmp.env_file.map(|s| expand_home(&s));
        let bind_path_file = tmp.bind_path_file.map(|s| expand_home(&s));

        GlobalConfig {
            store_dir,
            oocana_dir,
            env_file,
            bind_path_file,
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
#[serde(from = "TmpGlobalConfig")]
pub struct GlobalConfig {
    pub store_dir: String,
    pub oocana_dir: String,
    pub env_file: Option<String>,
    pub bind_path_file: Option<String>,
    pub search_paths: Option<Vec<String>>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        TmpGlobalConfig::default().into()
    }
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
struct TmpRunConfig {
    #[serde(default = "default_broker")]
    pub broker: String,
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}

impl Default for TmpRunConfig {
    fn default() -> Self {
        TmpRunConfig {
            broker: default_broker(),
            exclude_packages: None,
            reporter: None,
            debug: None,
            extra: None,
        }
    }
}

impl From<TmpRunConfig> for RunConfig {
    fn from(tmp: TmpRunConfig) -> Self {
        let extra = tmp.extra.map(|e| RunExtraConfig {
            search_paths: e.search_paths,
        });

        let exclude_packages = tmp.exclude_packages.map(|s| {
            s.into_iter()
                .map(|s| expand_home(&s))
                .collect::<Vec<String>>()
        });

        RunConfig {
            broker: tmp.broker,
            exclude_packages,
            reporter: tmp.reporter,
            debug: tmp.debug,
            extra,
        }
    }
}

impl Default for RunConfig {
    fn default() -> Self {
        TmpRunConfig::default().into()
    }
}

fn default_broker() -> String {
    "127.0.0.1:47688".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(from = "TmpRunConfig")]
pub struct RunConfig {
    pub broker: String,
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}
