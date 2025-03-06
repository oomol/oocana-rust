use serde::{Deserialize, Serialize};

use super::block::AppletBlock;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppletExecutorOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<u64>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Applet {
    #[serde(default)]
    pub executor: Option<AppletExecutorOptions>,
    #[serde(default)]
    pub blocks: Option<Vec<AppletBlock>>,
}
