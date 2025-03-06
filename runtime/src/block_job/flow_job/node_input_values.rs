use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::Write,
    path::PathBuf,
    sync::Arc,
};

use manifest_meta::{HandleName, Node, NodeId};

use utils::output::OutputValue;

pub type InputValueQueue = VecDeque<Arc<OutputValue>>;
pub type InputMapQueue = HashMap<HandleName, InputValueQueue>;

type InputValues = HashMap<HandleName, Arc<OutputValue>>;

/// Values are collected for each Node handle before starting a Node job
pub struct NodeInputValues {
    store: HashMap<NodeId, InputMapQueue>,
    last_values: Option<HashMap<NodeId, InputMapQueue>>,
}

impl NodeInputValues {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            last_values: Some(HashMap::new()),
        }
    }

    pub fn merge_input_values(&mut self, input_values: String) {
        match serde_json::from_str::<HashMap<NodeId, InputMapQueue>>(&input_values) {
            Ok(merge_values) => {
                for (node_id, input_map_queue) in merge_values {
                    self.store.insert(node_id, input_map_queue);
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize: {:?}", e);
            }
        }
    }

    pub fn recover_from(path: PathBuf) -> Self {
        if let Ok(file) = File::open(path) {
            let reader = std::io::BufReader::new(file);

            match serde_json::from_reader(reader) {
                Ok(store) => Self {
                    store,
                    last_values: None,
                },
                Err(e) => {
                    eprintln!("Failed to deserialize: {:?}", e);
                    Self {
                        store: HashMap::new(),
                        last_values: None,
                    }
                }
            }
        } else {
            Self {
                store: HashMap::new(),
                last_values: None,
            }
        }
    }

    pub fn insert(&mut self, node_id: NodeId, handle_name: HandleName, value: Arc<OutputValue>) {
        self.store
            .entry(node_id)
            .or_default()
            .entry(handle_name)
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

                if let Some(ref input_values) = self.store.get(node.node_id()) {
                    if input_values.get(&handle.handle).is_none()
                        || input_values.get(&handle.handle).unwrap().is_empty()
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

    pub fn save_last_value(&self, path: PathBuf) -> Result<(), String> {
        if let Some(last_values) = &self.last_values {
            // save hash map to file
            let json_string = serde_json::to_string(&last_values)
                .map_err(|e| format!("failed to serialize {}", e))?;

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

    pub fn take(&mut self, node: &Node) -> Option<InputValues> {
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
                .filter(|handle| !node.has_from(&handle))
                .collect();

            for handle in no_connection_handles {
                if let Some(def) = inputs_def.get(handle) {
                    if let Some(value) = &def.value {
                        value_map.insert(
                            handle.to_owned(),
                            Arc::new(OutputValue {
                                value: value.clone(),
                                sender: None,
                            }),
                        );
                    }
                }
            }
        }
        if value_map.is_empty() {
            None
        } else {
            if let Some(last_value) = &mut self.last_values {
                // TODO: 需要确认，这个 last_value 会不会影响到 value 的 drop 时机。
                // convert InputValues InputMapQueue
                // 未来可以考虑更改内部实现，不再需要这种转换
                let mut value_map_queue: InputMapQueue = HashMap::new();
                for (handle, value) in &value_map {
                    value_map_queue.insert(handle.to_owned(), VecDeque::from(vec![value.clone()]));
                }
                last_value.insert(node.node_id().to_owned(), value_map_queue);
            }
            Some(value_map)
        }
    }
}
