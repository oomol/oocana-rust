use std::path::PathBuf;

use tracing::warn;
use uuid::Uuid;

use crate::flow_job::{
    node_input_values::CacheMetaMap, node_input_values::CacheMetaMapExt, NodeInputValues,
};

pub fn get_flow_cache_path(flow: &str) -> Option<PathBuf> {
    utils::cache::cache_meta_file_path().and_then(|path| {
        CacheMetaMap::load(path)
            .ok()
            .and_then(|meta| meta.get(flow).map(|cache_path| cache_path.into()))
    })
}

pub(crate) fn save_flow_cache(node_input_values: &NodeInputValues, flow: &str) {
    if let Some(cache_path) = get_flow_cache_path(flow) {
        if let Err(e) = node_input_values.save_cache(cache_path) {
            warn!("failed to save cache: {}", e);
        }
    } else if let Some(meta_path) = utils::cache::cache_meta_file_path() {
        let cache_path = utils::cache::cache_dir()
            .unwrap_or(std::env::temp_dir())
            .join(Uuid::new_v4().to_string() + ".json");

        if let Err(e) = node_input_values.save_cache(cache_path.clone()) {
            warn!("failed to save cache: {}", e);
            return;
        }

        let mut meta = CacheMetaMap::load(meta_path.clone()).unwrap_or_default();
        if let Some(path) = cache_path.to_str() {
            meta.insert(flow.to_owned(), path.to_string());
            if let Err(e) = meta.save(meta_path) {
                warn!("failed to save cache meta: {}", e);
            }
        }
    }
}
