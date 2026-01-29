use crate::path::expand_home;
use serde::{Deserialize, Deserializer, Serialize};

fn default_store_dir() -> String {
    expand_home("~/.oomol-studio/oocana")
}

fn default_oocana_dir() -> String {
    expand_home("~/.oocana")
}

fn default_registry_store_file() -> String {
    expand_home("~/.registry/package_store.json")
}

fn deserialize_expand_home<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(expand_home(&s))
}

fn deserialize_expand_home_option<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.map(|s| expand_home(&s)))
}

fn deserialize_expand_home_vec_option<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<Vec<String>>::deserialize(deserializer)?;
    Ok(opt.map(|paths| paths.into_iter().map(|s| expand_home(&s)).collect()))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GlobalConfig {
    #[serde(default = "default_store_dir", deserialize_with = "deserialize_expand_home")]
    pub store_dir: String,
    #[serde(default = "default_oocana_dir", deserialize_with = "deserialize_expand_home")]
    pub oocana_dir: String,
    #[serde(
        default = "default_registry_store_file",
        deserialize_with = "deserialize_expand_home"
    )]
    pub registry_store_file: String,
    #[serde(default, deserialize_with = "deserialize_expand_home_option")]
    pub env_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_expand_home_option")]
    pub bind_path_file: Option<String>,
    #[serde(default, deserialize_with = "deserialize_expand_home_vec_option")]
    pub search_paths: Option<Vec<String>>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        GlobalConfig {
            store_dir: default_store_dir(),
            oocana_dir: default_oocana_dir(),
            registry_store_file: default_registry_store_file(),
            env_file: None,
            bind_path_file: None,
            search_paths: None,
        }
    }
}
