use std::collections::HashMap;

use crate::injection_store::with_injection_store;
use crate::layer::{create_layer, random_name};
use serde::{Deserialize, Serialize};
use utils::error::Result;

#[derive(Serialize, Deserialize, Clone)]
pub struct InjectionLayer {
    pub flow_path: String,
    pub scripts: Vec<String>,
    pub package_path: String,
    pub package_version: String,
    pub layer_name: String,
}

static INJECTION_LAYER_PREFIX: &str = "injection";

impl InjectionLayer {
    pub fn new(
        flow_path: String,
        scripts: Vec<String>,
        package_path: String,
        package_version: String,
    ) -> Self {
        let injection_layer_name = random_name(INJECTION_LAYER_PREFIX);

        // TODO: 考虑失败的情况
        let _ = create_layer(&injection_layer_name);

        InjectionLayer {
            flow_path,
            scripts,
            package_path,
            package_version,
            layer_name: injection_layer_name.to_owned(),
        }
    }

    pub fn is_equal_scripts(&self, scripts: Vec<String>) -> bool {
        let set_a = self
            .scripts
            .iter()
            .collect::<std::collections::HashSet<_>>();
        let set_b = scripts.iter().collect::<std::collections::HashSet<_>>();
        set_a == set_b
    }

    pub fn save_to_store(&self) -> Result<()> {
        let layer = self.clone();
        with_injection_store(|store| {
            let entry = store
                .flow_injection
                .entry(layer.flow_path.clone())
                .or_insert(HashMap::new());
            entry.insert(layer.package_path.clone(), layer);
            Ok(())
        })
    }
}
