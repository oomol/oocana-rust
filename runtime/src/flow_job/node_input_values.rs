use std::{
    cmp::min,
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::Write,
    path::PathBuf,
    sync::Arc,
};

use manifest_meta::{HandleName, Node, NodeId};
use tracing::warn;

use utils::error::Result;
use utils::output::OutputValue;

pub type InputValueQueue = VecDeque<Arc<OutputValue>>;
pub type InputMap = HashMap<HandleName, InputValueQueue>;
type NodeInputStore = HashMap<NodeId, InputMap>;

type InputValues = HashMap<HandleName, Arc<OutputValue>>;

/// Values are collected for each Node handle before starting a Node job
pub struct NodeInputValues {
    store: NodeInputStore,
    // used to store last values for each node input when `remember` is true
    memory_store: NodeInputStore,
    cache_value_store: Option<NodeInputStore>,
}

impl NodeInputValues {
    pub fn new(save_cache: bool) -> Self {
        Self {
            store: HashMap::new(),
            memory_store: HashMap::new(),
            cache_value_store: if save_cache {
                Some(HashMap::new())
            } else {
                None
            },
        }
    }

    pub fn recover_from(path: PathBuf, save_cache: bool) -> Self {
        let last_values = if save_cache {
            Some(HashMap::new())
        } else {
            None
        };

        if let Ok(file) = File::open(path) {
            let reader = std::io::BufReader::new(file);

            match serde_json::from_reader::<_, NodeInputStore>(reader) {
                Ok(store) => Self {
                    store: store.clone(),
                    memory_store: HashMap::new(),
                    cache_value_store: Some(store),
                },
                Err(e) => {
                    warn!("Failed to deserialize: {:?}", e);
                    Self {
                        store: HashMap::new(),
                        memory_store: HashMap::new(),
                        cache_value_store: last_values,
                    }
                }
            }
        } else {
            Self {
                store: HashMap::new(),
                memory_store: HashMap::new(),
                cache_value_store: last_values,
            }
        }
    }

    pub fn insert(&mut self, node_id: NodeId, handle_name: HandleName, value: Arc<OutputValue>) {
        self.store
            .entry(node_id.clone())
            .or_default()
            .entry(handle_name.clone())
            .or_default()
            .push_back(Arc::clone(&value));
    }

    pub fn is_node_fulfill(&self, node: &Node) -> bool {
        for (handle, input) in node.inputs() {
            if input.from.as_ref().is_none_or(|f| f.is_empty()) && input.value.is_some() {
                continue;
            }

            let no_handle_value = self
                .store
                .get(node.node_id())
                .and_then(|m| m.get(handle))
                .is_none_or(|v| v.is_empty());
            let no_memory_value = self
                .memory_store
                .get(node.node_id())
                .and_then(|m| m.get(handle))
                .is_none_or(|v| v.is_empty());
            if no_handle_value && no_memory_value {
                return false;
            }
        }

        true
    }

    pub fn node_has_input(&self, node: &Node, handle_name: &HandleName) -> bool {
        if let Some(input) = node.inputs().get(handle_name) {
            if input.from.as_ref().is_none_or(|f| f.is_empty()) && input.value.is_some() {
                return true;
            }
        }

        if let Some(input_values) = self.store.get(node.node_id()) {
            if let Some(values) = input_values.get(handle_name) {
                return !values.is_empty();
            }
        }
        false
    }

    pub fn node_pending_fulfill(&self, node: &Node) -> usize {
        if let Some(input_values) = self.store.get(node.node_id()) {
            let mut count = usize::MAX;
            for (h, v) in input_values.iter() {
                if need_remember_value(node, h) {
                    continue;
                }
                count = min(count, v.len())
            }
            return count;
        }
        0
    }

    pub fn save_cache(&self, path: PathBuf) -> Result<(), String> {
        if let Some(last_values) = &self.cache_value_store {
            // save hash map to file
            let json_string = serde_json::to_string(&last_values)
                .map_err(|e| format!("failed to serialize {}", e))?;

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create dir {}", e))?;
            }

            let mut file =
                File::create(path).map_err(|e| format!("failed to create file {}", e))?;
            file.write_all(json_string.as_bytes())
                .map_err(|e| format!("failed to write file {}", e))?;
        }
        Ok(())
    }

    pub fn remove_input_values(&mut self, node: &Node, from_nodes: &HashSet<NodeId>) {
        if let Some(inputs_map) = self.store.get_mut(node.node_id()) {
            for (handle, node_input) in node.inputs() {
                for from in node_input.from.iter().flatten() {
                    if let manifest_meta::HandleSource::NodeOutput { node_id, .. } = from {
                        if from_nodes.contains(node_id) {
                            inputs_map.remove(handle);
                        }
                    }
                }
            }
        }
    }

    fn remember_value(&mut self, node_id: &NodeId, handle: &HandleName, value: Arc<OutputValue>) {
        let vec = self
            .memory_store
            .entry(node_id.clone())
            .or_default()
            .entry(handle.to_owned())
            .or_default();

        vec.clear();
        vec.push_back(Arc::clone(&value));
    }

    pub fn take(&mut self, node: &Node) -> Option<InputValues> {
        let mut value_map: InputValues = HashMap::new();
        let mut to_remember: Vec<(HandleName, Arc<OutputValue>)> = Vec::new();
        let node_id = node.node_id();

        if let Some(input_values) = self.store.get_mut(node_id) {
            for (handle, values) in input_values {
                // this is a workaround, the best way is when flow or block is edited, clear the cache or run flow without `cache`
                if !node.has_connection(handle)
                    && node.inputs().get(handle).is_some_and(|i| i.value.is_some())
                {
                    warn!("Node {} handle {} has no connection with a static value exist. oocana will use handle's static value instead of the value in node store to avoid cache effect.", node_id, handle);
                    continue;
                }

                if let Some(first) = values.pop_front() {
                    value_map.insert(handle.to_owned(), Arc::clone(&first));

                    if values.is_empty() && need_remember_value(node, handle) {
                        to_remember.push((handle.to_owned(), Arc::clone(&first)));
                    }
                } else if let Some(remembered_value) = self
                    .memory_store
                    .get(node_id)
                    .and_then(|m| m.get(handle))
                    .and_then(|v| v.front())
                {
                    value_map.insert(handle.to_owned(), Arc::clone(remembered_value));
                }
            }
        }

        for (handle, value) in to_remember {
            self.remember_value(node_id, &handle, value);
        }

        for (handle, input) in node.inputs() {
            if value_map.contains_key(handle) {
                continue;
            }

            if input.from.as_ref().is_none_or(|f| f.is_empty()) {
                if let Some(value) = input.value.as_ref() {
                    value_map.insert(
                        handle.to_owned(),
                        Arc::new(OutputValue {
                            value: value.clone().unwrap_or(serde_json::Value::Null),
                            cacheable: true,
                        }),
                    );
                }
                continue;
            }
        }

        for (handle, value) in value_map.iter() {
            if !value.cacheable {
                continue;
            }

            if let Some(last_values) = &mut self.cache_value_store {
                let vec = last_values
                    .entry(node_id.to_owned())
                    .or_default()
                    .entry(handle.to_owned())
                    .or_default();
                vec.clear();
                vec.push_back(value.clone());
            }
        }

        if value_map.is_empty() {
            None
        } else {
            Some(value_map)
        }
    }
}

fn need_remember_value(node: &Node, handle: &HandleName) -> bool {
    if let Some(inputs_def) = node.inputs_def() {
        if let Some(def) = inputs_def.get(handle) {
            return def.remember;
        }
    }
    false
}

// key 是 flow path，value 是 flow path 对应缓存文件
pub type CacheMetaMap = HashMap<String, String>;

pub trait CacheMetaMapExt {
    fn save(&self, path: PathBuf) -> Result<()>
    where
        Self: Sized;
    fn load(path: PathBuf) -> Result<Self>
    where
        Self: Sized;
}

impl CacheMetaMapExt for CacheMetaMap {
    fn load(path: PathBuf) -> Result<Self>
    where
        Self: Sized,
    {
        let store = if let Ok(file) = File::open(path.clone()) {
            let reader = std::io::BufReader::new(file);

            serde_json::from_reader(reader)
                .map_err(|e| warn!("Failed to deserialize cache meta from: {:?} {:?}", path, e))
                .unwrap_or_else(|_| HashMap::new())
        } else {
            HashMap::new()
        };

        Ok(store)
    }

    fn save(&self, path: PathBuf) -> Result<()> {
        let json_string = serde_json::to_string(&self)?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?
        }

        let mut file = File::create(path)?;

        file.write_all(json_string.as_bytes())?;
        Ok(())
    }
}
