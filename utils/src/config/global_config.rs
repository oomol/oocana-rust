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
