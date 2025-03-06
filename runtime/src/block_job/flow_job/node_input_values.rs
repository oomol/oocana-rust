use std::{collections::HashMap, sync::Arc};

use manifest_meta::{HandleName, InputHandleCache, JsonValue, Node, NodeId};

pub type InputValues = HashMap<HandleName, Arc<JsonValue>>;

/// Values are collected for each Node handle before starting a Node job
pub struct NodeInputValues {
    store: HashMap<NodeId, InputValues>,
}

impl NodeInputValues {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub fn insert(&mut self, node_id: NodeId, handle_name: HandleName, value: Arc<JsonValue>) {
        self.store
            .entry(node_id)
            .or_default()
            .insert(handle_name, value);
    }

    pub fn peek(&self, node_id: &NodeId) -> Option<&InputValues> {
        self.store.get(node_id)
    }

    pub fn take(&mut self, node: &Node) -> Option<InputValues> {
        if let Some(inputs_def) = node.inputs_def() {
            if inputs_def.values().any(|def| def.cache.is_enabled()) {
                let mut values: InputValues = HashMap::new();
                for (handle, def) in inputs_def {
                    match &def.cache {
                        InputHandleCache::Bool(true) => {
                            if let Some(input_values) = self.store.get(node.node_id()) {
                                if let Some(value) = input_values.get(handle) {
                                    values.insert(handle.to_owned(), Arc::clone(value));
                                }
                            }
                        }
                        InputHandleCache::Bool(false) => {
                            if let Some(input_values) = self.store.get_mut(node.node_id()) {
                                if let Some(value) = input_values.remove(handle) {
                                    values.insert(handle.to_owned(), value);
                                }
                            }
                        }
                        InputHandleCache::InitialValue { initial_value } => {
                            if let Some(input_values) = self.store.get_mut(node.node_id()) {
                                values.insert(
                                    handle.to_owned(),
                                    Arc::clone(input_values.get(handle).unwrap_or(initial_value)),
                                );
                            } else {
                                values.insert(handle.to_owned(), Arc::clone(initial_value));
                            }
                        }
                    }
                }
                return Some(values);
            }
        }
        self.store.remove(node.node_id())
    }
}
