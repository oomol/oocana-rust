use std::collections::HashMap;

use crate::injection_store::{injection_store_path, load_injection_store};
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
        let file = injection_store_path()?;
        let mut store = load_injection_store()?;
        let key = self.flow_path.clone();
        let entry = store.flow_injection.entry(key).or_insert(HashMap::new());
        entry.insert(self.package_path.clone(), self.clone());
        let f =
            std::fs::File::create(&file).map_err(|e| format!("Failed to create file: {:?}", e))?;
        let writer = std::io::BufWriter::new(f);
        serde_json::to_writer_pretty(writer, &store)
            .map_err(|e| format!("Failed to serialize: {:?}", e))?;
        Ok(())
    }
}
