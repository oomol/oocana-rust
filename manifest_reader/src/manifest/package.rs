use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
pub struct Scripts {
    pub bootstrap: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PackageMeta {
    pub name: Option<String>,
    pub version: Option<String>,
    pub exports: Option<HashMap<String, String>>,
    pub scripts: Option<Scripts>,
    pub dependencies: Option<HashMap<String, String>>,
}
