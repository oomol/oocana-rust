use crate::path::expand_home;
use serde::{Deserialize, Deserializer, Serialize};

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
pub struct RunExtraConfig {
    #[serde(default, deserialize_with = "deserialize_expand_home_vec_option")]
    pub search_paths: Option<Vec<String>>,
}

fn default_broker() -> String {
    "127.0.0.1:47688".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunConfig {
    #[serde(default = "default_broker")]
    pub broker: String,
    #[serde(default, deserialize_with = "deserialize_expand_home_vec_option")]
    pub exclude_packages: Option<Vec<String>>,
    pub reporter: Option<bool>,
    pub debug: Option<bool>,
    pub extra: Option<RunExtraConfig>,
}

impl Default for RunConfig {
    fn default() -> Self {
        RunConfig {
            broker: default_broker(),
            exclude_packages: None,
            reporter: None,
            debug: None,
            extra: None,
        }
    }
}
