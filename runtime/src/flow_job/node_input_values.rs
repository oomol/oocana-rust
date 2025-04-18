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
    signal_store: HashMap<NodeId, HashMap<NodeId, VecDeque<i32>>>,
    last_values: Option<NodeInputStore>,
}

impl NodeInputValues {
    pub fn new(save_cache: bool) -> Self {
        Self {
            store: HashMap::new(),
            signal_store: HashMap::new(),
            last_values: if save_cache {
                Some(HashMap::new())
            } else {
                None
            },
        }
    }

    pub fn merge_input_values(&mut self, input_values: String) {
        match serde_json::from_str::<NodeInputStore>(&input_values) {
            Ok(merge_values) => {
                for (node_id, input_map_queue) in merge_values {
                    self.store.insert(node_id, input_map_queue);
                }
            }
            Err(e) => {
                warn!("Failed to deserialize: {:?}", e);
            }
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
                    signal_store: HashMap::new(),
                    last_values: Some(store),
                },
                Err(e) => {
                    warn!("Failed to deserialize: {:?}", e);
                    Self {
                        store: HashMap::new(),
                        signal_store: HashMap::new(),
                        last_values,
                    }
                }
            }
        } else {
            Self {
                store: HashMap::new(),
                signal_store: HashMap::new(),
                last_values,
            }
        }
    }

    pub fn insert_value(
        &mut self,
        node_id: NodeId,
        handle_name: HandleName,
        value: Arc<OutputValue>,
    ) {
        self.store
            .entry(node_id.clone())
            .or_default()
            .entry(handle_name.clone())
            .or_default()
            .push_back(Arc::clone(&value));

        if !value.cacheable {
            return;
        }

        if let Some(last_values) = &mut self.last_values {
            // replace VecDeque with Var not push_back
            let vec = last_values
                .entry(node_id)
                .or_default()
                .entry(handle_name)
                .or_default();
            vec.clear();
            vec.push_back(value);
        }
    }

    pub fn insert_signal(&mut self, node_id: NodeId, signal_node_id: NodeId, value: i32) {
        self.signal_store
            .entry(node_id)
            .or_default()
            .entry(signal_node_id)
            .or_default()
            .push_back(value);
    }

    pub fn is_node_fulfill(&self, node: &Node) -> bool {
        if let Some(inputs_def) = node.inputs_def() {
            for handle in inputs_def.values() {
                // has_from 为 false，说明没有连线。
                if !node.has_from(&handle.handle) {
                    // 无连线有 value 认为该 input 已满足。
                    if handle.value.is_some() {
                        continue;
                    }

                    // 未来有其他配置项，作用于无连线状态时，在这里更新即可。现在无连线，同时无 value，继续往下走。会一直卡住。
                    return false;
                }

                if let Some(input_values) = self.store.get(node.node_id()) {
                    if input_values.get(&handle.handle).is_none()
                        || input_values
                            .get(&handle.handle)
                            .is_some_and(|v| v.is_empty())
                    {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        if let Some(after) = node.after() {
            for node_id in after {
                if let Some(signal_map) = self.signal_store.get(node.node_id()) {
                    if signal_map.get(node_id).is_none()
                        || signal_map.get(node_id).is_some_and(|v| v.is_empty())
                    {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }

        true
    }

    pub fn node_has_input(&self, node: &Node, handle_name: HandleName) -> bool {
        if let Some(inputs_def) = node.inputs_def() {
            // 没有连线的话，查看是否有 value 值。有值认为也存在。
            if !node.has_from(&handle_name) {
                return inputs_def
                    .get(&handle_name)
                    .map(|f| f.value.is_some())
                    .unwrap_or(false);
            }
        }

        if let Some(input_values) = self.store.get(node.node_id()) {
            if let Some(values) = input_values.get(&handle_name) {
                return !values.is_empty();
            }
        }
        false
    }

    pub fn node_pending_fulfill(&self, node_id: &NodeId) -> usize {
        if let Some(input_values) = self.store.get(node_id) {
            let mut count = usize::MAX;
            for (_, v) in input_values.iter() {
                count = min(count, v.len())
            }
            return count;
        }
        0
    }

    pub fn save_last_value(&self, path: PathBuf) -> Result<(), String> {
        if let Some(last_values) = &self.last_values {
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
            if let Some(froms) = node.from() {
                for (handle, froms) in froms {
                    for from in froms {
                        if let manifest_meta::HandleFrom::FromNodeOutput { node_id, .. } = from {
                            if from_nodes.contains(node_id) {
                                inputs_map.remove(handle);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn take_value(&mut self, node: &Node) -> Option<InputValues> {
        let mut value_map: InputValues = HashMap::new();

        if let Some(input_values) = self.store.get_mut(node.node_id()) {
            for (handle, values) in input_values {
                if let Some(first) = values.pop_front() {
                    value_map.insert(handle.to_owned(), Arc::clone(&first));
                }
            }
        }
        if let Some(inputs_def) = node.inputs_def() {
            // filter inputs_def which has no connected handle input
            let no_connection_handles: Vec<&HandleName> = inputs_def
                .keys()
                .filter(|handle| !node.has_from(handle))
                .collect();

            for handle in no_connection_handles {
                if let Some(def) = inputs_def.get(handle) {
                    if let Some(value) = &def.value {
                        value_map.insert(
                            handle.to_owned(),
                            Arc::new(OutputValue {
                                value: value.clone().unwrap_or(serde_json::Value::Null),
                                cacheable: true,
                            }),
                        );
                    }
                }
            }
        }
        if value_map.is_empty() {
            None
        } else {
            Some(value_map)
        }
    }
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
