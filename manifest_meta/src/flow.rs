use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Formatter},
    path::PathBuf,
    sync::Arc,
};

use manifest_reader::{
    manifest::{
        self, HandleName, InputDefPatch, InputHandle, InputHandles, OutputHandle, OutputHandles,
    },
    path_finder::{calculate_block_value_type, find_package_file, BlockPathFinder, BlockValueType},
    reader::read_package,
    JsonValue,
};

use crate::{
    node::{
        common::NodeInput,
        subflow::{Slot, SubflowSlot, TaskSlot},
    },
    scope::{calculate_running_target, BlockScope, RunningTarget},
};

use tracing::warn;
use utils::error::Result;

use crate::{
    block_resolver::{package_path, BlockResolver},
    connections::Connections,
    node::{ServiceNode, TaskNode},
    HandlesFroms, HandlesTos, Node, NodeId, SlotNode, SubflowNode,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum InjectionTarget {
    Package(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InjectionNode {
    pub node_id: NodeId,
    /// node 文件的绝对路径，后续会将这个文件所在目录，注入到 package 中
    pub absolute_entry: PathBuf,
    /// node 文件的相对路径，在注入 package 后，就是相对于 package 的路径
    pub relative_entry: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InjectionMeta {
    pub nodes: Vec<InjectionNode>,
    /// value 设置为数组，方便未来如果每个支持每个小脚本有独立的注入代码(也可以在 injectionNode 里面设置)
    pub scripts: Option<Vec<String>>,
    pub package_version: String,
}

pub type InjectionStore = HashMap<InjectionTarget, InjectionMeta>;

#[derive(Debug, Clone)]
pub struct SubflowBlock {
    pub description: Option<String>,
    pub nodes: HashMap<NodeId, Node>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub path: PathBuf,
    pub path_str: String,
    /// Flow inputs to in-flow nodes
    pub flow_inputs_tos: HandlesTos,
    /// Flow outputs from in-flow nodes
    pub flow_outputs_froms: HandlesFroms,
    pub package_path: Option<PathBuf>,
    pub injection_store: Option<InjectionStore>,
}

#[derive(Hash, PartialEq, Eq, Debug)]
pub struct ServiceQueryResult {
    entry: Option<String>,
    dir: String,
    package: Option<String>,
    service_hash: String,
    is_global: bool,
}

impl fmt::Display for ServiceQueryResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut msg = format!(
            "{{ \"dir\": {:?}, \"service_hash\": {:?}, \"is_global\": {:?}",
            self.dir, self.service_hash, self.is_global
        );
        if let Some(entry) = &self.entry {
            msg.push_str(&format!(", \"entry\": {:?}", entry));
        }
        if let Some(package) = &self.package {
            msg.push_str(&format!(", \"package\": {:?}", package));
        }
        msg.push_str(" }");
        write!(f, "{msg}")
    }
}

pub static RUNTIME_HANDLE_PREFIX: &str = "runtime";

pub fn generate_runtime_handle_name(node_id: &str, handle: &HandleName) -> HandleName {
    format!("{}::{}-{}", RUNTIME_HANDLE_PREFIX, node_id, handle).into()
}

pub fn generate_node_inputs(
    inputs_def: &Option<InputHandles>,
    froms: &Option<HandlesFroms>,
    node_inputs_from: &Option<Vec<manifest::NodeInputFrom>>,
    // TODO: patch can be get from node_inputs_from. get patches from node_inputs_from
    patches: &Option<HashMap<HandleName, Vec<InputDefPatch>>>,
    node_id: &NodeId,
) -> HashMap<HandleName, NodeInput> {
    let mut inputs = HashMap::new();
    // inputs_def should contain all input handles. in the future, maybe has input from not in inputs_def.
    if let Some(def) = inputs_def {
        for (handle, input_def) in def.iter() {
            let from = froms.as_ref().and_then(|froms| froms.get(handle)).cloned();
            let patch = patches
                .as_ref()
                .and_then(|patches| patches.get(handle))
                .cloned();
            let value = if from.as_ref().is_some_and(|f| f.len() == 1)
                && matches!(
                    from.as_ref().unwrap()[0],
                    crate::HandleFrom::FromValue { .. }
                ) {
                from.as_ref().and_then(|f| {
                    f.first().and_then(|hf| {
                        if let crate::HandleFrom::FromValue { value } = hf {
                            value.clone()
                        } else {
                            None
                        }
                    })
                })
            } else {
                node_inputs_from.as_ref().and_then(|inputs_from| {
                    inputs_from.iter().find_map(|input_from| {
                        if input_from.handle == *handle {
                            input_from.value.clone()
                        } else {
                            None
                        }
                    })
                })
            };

            // remove from value node connection
            let connection_from = from.map(|f| {
                f.iter()
                    .filter_map(|ff| match ff {
                        crate::HandleFrom::FromFlowInput { input_handle } => {
                            Some(crate::node::HandleSource::FlowInput {
                                input_handle: input_handle.clone(),
                            })
                        }
                        crate::HandleFrom::FromNodeOutput {
                            node_id,
                            output_handle,
                        } => Some(crate::node::HandleSource::NodeOutput {
                            node_id: node_id.clone(),
                            output_handle: output_handle.clone(),
                        }),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            });

            if value.is_none() && connection_from.as_ref().is_some_and(|f| f.is_empty()) {
                warn!("node id ({}) handle: ({}) has no connection and has no value. This node won't run.",  node_id, handle);
                if input_def.value.is_some() {
                    warn!(
                        "node id ({}) handle: ({}) has value in inputs_def but is not used. For now the inputs_def's value will be ignored.",
                        node_id,
                        handle
                    );
                }
            }

            let serialize_for_cache = if let Some(node_inputs_from) = node_inputs_from {
                node_inputs_from.iter().any(|input_from| {
                    input_from.handle == *handle && input_from.serialize_for_cache
                })
            } else {
                false
            };

            inputs.insert(
                handle.to_owned(),
                NodeInput {
                    def: InputHandle {
                        _deserialize_from_cache: serialize_for_cache,
                        ..input_def.clone()
                    },
                    patch,
                    value,
                    sources: connection_from,
                    serialize_for_cache,
                },
            );
        }
    }
    inputs
}

pub type MergeInputsValue = HashMap<NodeId, HashMap<HandleName, JsonValue>>;

impl SubflowBlock {
    pub fn has_slot(&self) -> bool {
        self.nodes
            .values()
            .any(|node| matches!(node, Node::Slot(_)))
    }

    pub fn merge_input_values(&mut self, inputs_value: MergeInputsValue) {
        for (node_id, merge_inputs) in inputs_value {
            if let Some(node) = self.nodes.get_mut(&node_id) {
                let mut inputs = node.inputs().clone();
                for (handle, value) in merge_inputs {
                    if !inputs.contains_key(&handle) {
                        warn!(
                            "won't merge handle `{}` to node `{}` because the handle does not exist in the node",
                            handle, node_id
                        );
                        continue;
                    }
                    inputs
                        .entry(handle)
                        .and_modify(|i| i.value = Some(Some(value)));
                }
                node.update_inputs(inputs);
            }
        }
    }

    pub fn query_inputs(&self) -> HashMap<NodeId, Vec<InputHandle>> {
        let mut inputs: HashMap<NodeId, Vec<InputHandle>> = HashMap::new();
        for (node_id, node) in &self.nodes {
            for input in node.inputs().values() {
                if input.sources.as_ref().is_some_and(|f| !f.is_empty()) {
                    continue; // skip if input has connection
                }

                // copy runtime value to input def when querying inputs
                let input_def = InputHandle {
                    value: input.value.clone(),
                    ..input.def.clone()
                };

                inputs.entry(node_id.clone()).or_default().push(input_def);
            }
        }
        inputs
    }

    pub fn update_node(&mut self, node_id: &NodeId, node: Node) {
        if let Some(existing_node) = self.nodes.get_mut(node_id) {
            *existing_node = node;
        } else {
            self.nodes.insert(node_id.clone(), node);
        }
    }

    pub fn update_flow_inputs_tos(
        &mut self,
        input_handle: &HandleName,
        handle_tos: &[crate::HandleTo],
    ) {
        self.flow_inputs_tos
            .entry(input_handle.to_owned())
            .or_default()
            .extend(handle_tos.iter().cloned());
    }

    pub fn from_manifest(
        manifest: manifest::SubflowBlock,
        flow_path: PathBuf,
        block_resolver: &mut BlockResolver,
        mut path_finder: BlockPathFinder,
    ) -> Result<Self> {
        let manifest::SubflowBlock {
            description,
            nodes,
            inputs_def,
            outputs_def,
            outputs_from,
            injection: scripts,
        } = manifest;

        // filter out ignored value nodes
        let value_nodes = nodes
            .iter()
            .filter_map(|node| match node {
                manifest::Node::Value(value_node) => Some(value_node.clone()),
                _ => None,
            })
            .filter(|n| !n.ignore)
            .collect::<Vec<_>>();
        let value_nodes_id = value_nodes
            .iter()
            .map(|n| n.node_id.clone())
            .collect::<HashSet<_>>();

        let find_value_node = |node_id: &NodeId| -> Option<manifest::ValueNode> {
            value_nodes.iter().find(|n| n.node_id == *node_id).cloned()
        };

        let nodes_in_flow: Vec<manifest::Node> = nodes
            .into_iter()
            .filter(|node| !node.should_ignore() && !value_nodes_id.contains(node.node_id()))
            .collect();

        let mut connections = Connections::new(
            nodes_in_flow
                .iter()
                .map(|n| n.node_id().to_owned())
                .collect(),
        );

        connections.parse_flow_outputs_from(outputs_from);

        for node in nodes_in_flow.iter() {
            connections.parse_node_inputs_from(
                node.node_id(),
                node.inputs_from(),
                &find_value_node,
                &inputs_def,
            );

            if matches!(node, manifest::Node::Subflow(ref subflow_node) if subflow_node.slots.as_ref().is_some_and(|s| !s.is_empty()))
            {
                if let manifest::Node::Subflow(subflow_node) = node {
                    if let Some(slots) = subflow_node.slots.as_ref() {
                        for provider in slots {
                            connections.parse_slot_inputs_from(
                                node.node_id(),
                                &provider.node_id(),
                                provider.inputs_from(),
                                &find_value_node,
                            );
                        }
                    }
                }
            }
        }

        let find_node = |node_id: &NodeId| -> Option<&manifest::Node> {
            nodes_in_flow.iter().find(|n| n.node_id() == node_id)
        };

        drop(value_nodes);
        drop(value_nodes_id);

        type PackageInjectionScripts = HashMap<PathBuf, Vec<String>>;
        let mut injection_scripts: PackageInjectionScripts = HashMap::new();
        if let Some(map) = scripts {
            for (pkg, script) in map {
                let mut pkg_file_path = path_finder.find_package_file_path(&pkg)?;
                pkg_file_path = pkg_file_path.parent().unwrap().to_path_buf();
                injection_scripts
                    .entry(pkg_file_path)
                    .or_default()
                    .push(script);
            }
        }

        type InjectionNodes = HashMap<InjectionTarget, HashSet<InjectionNode>>;
        let mut injection_nodes: InjectionNodes = HashMap::new();

        let mut new_nodes: HashMap<NodeId, Node> = HashMap::new();

        for node in &nodes_in_flow {
            match node {
                manifest::Node::Subflow(subflow_node) => {
                    let mut flow = block_resolver
                        .resolve_flow_block(&subflow_node.subflow, &mut path_finder)?;
                    let subflow_inputs_def = flow.inputs_def.clone();
                    let subflow_inputs_def_patch = get_inputs_def_patch(&subflow_node.inputs_from);
                    let to = connections.node_outputs_tos.remove(&subflow_node.node_id);

                    let mut addition_subflow_inputs_def: InputHandles = HashMap::new();

                    let running_target = calculate_running_target(
                        node,
                        &None,
                        &flow.package_path,
                        calculate_block_value_type(&subflow_node.subflow),
                    );

                    let running_scope = match running_target {
                        RunningTarget::Inherit => BlockScope::default(),
                        RunningTarget::Package {
                            package_path: path,
                            node_id,
                            pkg_name,
                            ..
                        } => BlockScope::Package {
                            name: pkg_name,
                            inject: false,
                            path,
                            node_id,
                        },
                        _ => {
                            warn!("subflow node injection not supported");
                            BlockScope::default()
                        }
                    };

                    let mut slot_blocks: HashMap<NodeId, Slot> = HashMap::new();
                    if let Some(slot_providers) = subflow_node.slots.as_ref() {
                        for provider in slot_providers {
                            match provider {
                                manifest::SlotProvider::Task(task_slot_provider) => {
                                    let task = block_resolver.resolve_task_node_block(
                                        manifest::TaskNodeBlock::File(
                                            task_slot_provider.task.to_owned(),
                                        ),
                                        &mut path_finder,
                                    )?;

                                    let scope = match calculate_block_value_type(
                                        &task_slot_provider.task,
                                    ) {
                                        BlockValueType::Pkg { pkg_name, .. } => {
                                            if let Some(package_path) = &task.package_path {
                                                BlockScope::Package {
                                                    name: pkg_name,
                                                    path: package_path.clone(),
                                                    inject: false,
                                                    node_id: None,
                                                }
                                            } else {
                                                BlockScope::Slot {}
                                            }
                                        }
                                        _ => BlockScope::Slot {},
                                    };

                                    let slot_block = Slot::Task(TaskSlot {
                                        slot_node_id: task_slot_provider.slot_node_id.to_owned(),
                                        task,
                                        scope,
                                    });
                                    slot_blocks.insert(
                                        task_slot_provider.slot_node_id.to_owned(),
                                        slot_block,
                                    );
                                }
                                manifest::SlotProvider::SlotFlow(slotflow_provider) => {
                                    // slotflow inputs def is not defined in the slotflow, we need to create it from slot node inputs_def
                                    // and merge all slotflow_provider.inputs_def into it.
                                    // it is user's responsibility to ensure that these inputs_def are not conflicting.
                                    let mut slotflow_inputs_def = flow
                                        .nodes
                                        .get(&slotflow_provider.slot_node_id)
                                        .and_then(|n| n.inputs_def());

                                    if let Some(additional_slotflow_inputs_def) =
                                        slotflow_provider.inputs_def.as_ref()
                                    {
                                        for input_def in additional_slotflow_inputs_def.iter() {
                                            // TODO: can add some warning if handle name is already defined
                                            if !slotflow_inputs_def
                                                .as_ref()
                                                .is_some_and(|d| d.contains_key(&input_def.handle))
                                            {
                                                slotflow_inputs_def
                                                    .get_or_insert_with(HashMap::new)
                                                    .insert(
                                                        input_def.handle.to_owned(),
                                                        input_def.clone(),
                                                    );
                                            }
                                        }
                                    }

                                    let slotflow = block_resolver.resolve_slot_flow_block(
                                        &slotflow_provider.slotflow,
                                        slotflow_inputs_def,
                                        &mut path_finder,
                                    )?;

                                    let scope = match calculate_block_value_type(
                                        &slotflow_provider.slotflow,
                                    ) {
                                        BlockValueType::Pkg { pkg_name, .. } => {
                                            if let Some(package_path) = &slotflow.package_path {
                                                BlockScope::Package {
                                                    name: pkg_name,
                                                    path: package_path.clone(),
                                                    inject: false,
                                                    node_id: None,
                                                }
                                            } else {
                                                BlockScope::Slot {}
                                            }
                                        }
                                        _ => BlockScope::Slot {},
                                    };

                                    let slot_block = Slot::Subflow(SubflowSlot {
                                        slot_node_id: slotflow_provider.slot_node_id.to_owned(),
                                        subflow: slotflow,
                                        scope,
                                    });

                                    // update slot node's inputs if slotflow_provider.inputs_def and inputs_from are defined.
                                    // we will generate the new input handle name as `runtime::slot_node_id::input_handle` format, so the slotflow_provider.inputs_def can be same as the slot node's inputs_def
                                    if let Some(Node::Slot(slot_node)) =
                                        flow.nodes.get(&slotflow_provider.slot_node_id)
                                    {
                                        let mut new_slot_node_inputs = slot_node.inputs.clone();
                                        let mut additional_flow_inputs_tos: HashMap<
                                            HandleName,
                                            Vec<crate::HandleTo>,
                                        > = HashMap::new();
                                        if let Some(additional_inputs_def) =
                                            &slotflow_provider.inputs_def
                                        {
                                            for input in additional_inputs_def.iter() {
                                                let runtime_handle_name =
                                                    generate_runtime_handle_name(
                                                        &slot_node.node_id,
                                                        &input.handle,
                                                    );

                                                // just avoid user add this format handle name to slot node inputs
                                                if new_slot_node_inputs.contains_key(&input.handle)
                                                {
                                                    tracing::warn!(
                                                        "slot node {} already has input {}",
                                                        slotflow_provider.slot_node_id,
                                                        input.handle
                                                    );
                                                    continue;
                                                }

                                                new_slot_node_inputs.insert(
                                                    input.handle.clone(),
                                                    NodeInput {
                                                        def: InputHandle {
                                                            remember: true,
                                                            is_additional: true,
                                                            ..input.clone()
                                                        },
                                                        patch: None,
                                                        value: None,
                                                        sources: None, // from will be added later
                                                        serialize_for_cache: false, // TODO: get serialize_for_cache from slotflow_provider.inputs_from
                                                    },
                                                );

                                                addition_subflow_inputs_def.insert(
                                                    runtime_handle_name.clone(),
                                                    InputHandle {
                                                        handle: runtime_handle_name.clone(),
                                                        is_additional: true,
                                                        ..input.clone()
                                                    },
                                                );

                                                if let Some(input_from) = slotflow_provider
                                                    .inputs_from
                                                    .as_ref()
                                                    .and_then(|inputs_from| {
                                                        inputs_from
                                                            .iter()
                                                            .find(|i| i.handle == input.handle)
                                                    })
                                                {
                                                    if let Some(value) = &input_from.value {
                                                        new_slot_node_inputs
                                                            .entry(input.handle.clone())
                                                            .and_modify(|node_input| {
                                                                node_input.value =
                                                                    Some(value.clone());
                                                            });
                                                    }

                                                    if input_from
                                                        .from_flow
                                                        .as_ref()
                                                        .is_some_and(|f| !f.is_empty())
                                                        || input_from
                                                            .from_node
                                                            .as_ref()
                                                            .is_some_and(|n| !n.is_empty())
                                                    {
                                                        new_slot_node_inputs
                                                            .entry(input.handle.clone())
                                                            .and_modify(|node_input| {
                                                                node_input
                                                                    .sources
                                                                    .get_or_insert_with(Vec::new)
                                                                    .push(
                                                                        crate::node::HandleSource::FlowInput {
                                                                            input_handle: runtime_handle_name.clone(),
                                                                        },
                                                                    );
                                                            });
                                                    }
                                                } else {
                                                    warn!(
                                                            "slot node {} input {} not found in inputs_from",
                                                            slotflow_provider.slot_node_id,
                                                            input.handle
                                                        );
                                                }

                                                additional_flow_inputs_tos
                                                    .entry(runtime_handle_name.to_owned())
                                                    .or_default()
                                                    .push(crate::HandleTo::ToNodeInput {
                                                        node_id: slot_node.node_id.clone(),
                                                        input_handle: input.handle.to_owned(),
                                                    });
                                            }

                                            let mut new_slot_node = slot_node.clone();
                                            {
                                                new_slot_node.inputs = new_slot_node_inputs;
                                            }

                                            // Cannot mutate inside Arc, so clone, update, and re-wrap if needed
                                            let mut flow_inner = (*flow).clone();
                                            flow_inner.update_node(
                                                &slotflow_provider.slot_node_id,
                                                Node::Slot(new_slot_node),
                                            );
                                            for (input_handle, handle_tos) in
                                                additional_flow_inputs_tos.iter()
                                            {
                                                flow_inner.update_flow_inputs_tos(
                                                    input_handle,
                                                    handle_tos,
                                                );
                                            }
                                            flow = Arc::new(flow_inner);
                                        }
                                    } else {
                                        warn!(
                                            "slot node not found: {}",
                                            slotflow_provider.slot_node_id
                                        );
                                    }

                                    slot_blocks.insert(
                                        slotflow_provider.slot_node_id.to_owned(),
                                        slot_block,
                                    );
                                }
                                manifest::SlotProvider::Subflow(subflow_provider) => {
                                    let slot_flow = block_resolver.resolve_flow_block(
                                        &subflow_provider.subflow,
                                        &mut path_finder,
                                    )?;

                                    if slot_flow.has_slot() {
                                        tracing::warn!("this subflow has slot node");
                                    }

                                    let scope =
                                        match calculate_block_value_type(&subflow_provider.subflow)
                                        {
                                            BlockValueType::Pkg { pkg_name, .. } => {
                                                if let Some(package_path) = &slot_flow.package_path
                                                {
                                                    BlockScope::Package {
                                                        name: pkg_name,
                                                        path: package_path.to_owned(),
                                                        inject: false,
                                                        node_id: None,
                                                    }
                                                } else {
                                                    BlockScope::Slot {}
                                                }
                                            }
                                            _ => BlockScope::Slot {},
                                        };

                                    let slot_block = Slot::Subflow(SubflowSlot {
                                        slot_node_id: subflow_provider.slot_node_id.to_owned(),
                                        subflow: slot_flow,
                                        scope,
                                    });

                                    slot_blocks.insert(
                                        subflow_provider.slot_node_id.to_owned(),
                                        slot_block,
                                    );
                                }
                            }
                        }
                    }

                    let inputs_def = if !addition_subflow_inputs_def.is_empty() {
                        let mut merged_inputs_def = subflow_inputs_def.clone().unwrap_or_default();
                        for (handle, input) in addition_subflow_inputs_def.iter() {
                            if !merged_inputs_def.contains_key(handle) {
                                merged_inputs_def.insert(
                                    handle.to_owned(),
                                    InputHandle {
                                        handle: handle.to_owned(),
                                        ..input.clone()
                                    },
                                );
                            }
                        }
                        Some(merged_inputs_def)
                    } else {
                        subflow_inputs_def
                    };

                    let from = connections.node_inputs_froms.remove(&subflow_node.node_id);
                    let inputs = generate_node_inputs(
                        &inputs_def,
                        &from,
                        &subflow_node.inputs_from,
                        &subflow_inputs_def_patch,
                        &subflow_node.node_id,
                    );

                    new_nodes.insert(
                        subflow_node.node_id.to_owned(),
                        Node::Flow(SubflowNode {
                            description: subflow_node.description.clone(),
                            to,
                            flow,
                            node_id: subflow_node.node_id.to_owned(),
                            timeout: subflow_node.timeout,
                            inputs,
                            concurrency: subflow_node.concurrency,
                            scope: running_scope,
                            slots: if slot_blocks.is_empty() {
                                None
                            } else {
                                Some(slot_blocks)
                            },
                        }),
                    );
                }
                manifest::Node::Service(service_node) => {
                    let service = block_resolver.resolve_service_node_block(
                        service_node.service.to_owned(),
                        &mut path_finder,
                    )?;
                    let inputs_def = service.inputs_def.clone();
                    let inputs_def_patch = get_inputs_def_patch(&service_node.inputs_from);
                    let from = connections.node_inputs_froms.remove(&service_node.node_id);

                    let inputs = generate_node_inputs(
                        &inputs_def,
                        &from,
                        &service_node.inputs_from,
                        &inputs_def_patch,
                        &service_node.node_id,
                    );

                    new_nodes.insert(
                        service_node.node_id.to_owned(),
                        Node::Service(ServiceNode {
                            description: service_node.description.clone(),
                            to: connections.node_outputs_tos.remove(&service_node.node_id),
                            node_id: service_node.node_id.to_owned(),
                            timeout: service_node.timeout,
                            block: service,
                            inputs,
                            concurrency: service_node.concurrency,
                        }),
                    );
                }
                manifest::Node::Value(_) => {}
                manifest::Node::Task(task_node) => {
                    let task_node_block = task_node.task.clone();

                    let task = block_resolver
                        .resolve_task_node_block(task_node_block.clone(), &mut path_finder)?;

                    let running_target = calculate_running_target(
                        node,
                        &task_node.inject,
                        &task.package_path,
                        task_node_block.block_type(),
                    );

                    let mut running_scope = match running_target {
                        RunningTarget::Inherit => BlockScope::default(),
                        RunningTarget::Package {
                            pkg_name,
                            package_path: path,
                            node_id,
                            ..
                        } => BlockScope::Package {
                            name: pkg_name,
                            inject: false,
                            path,
                            node_id,
                        },
                        RunningTarget::Node(node_id) => match find_node(&node_id) {
                            Some(_) => BlockScope::Flow {
                                node_id: Some(node_id),
                            },
                            None => {
                                warn!("target node not found: {:?}", node_id);
                                BlockScope::default()
                            }
                        },
                        RunningTarget::InjectPackage { pkg_name } => {
                            let pkg_path = path_finder
                                .find_package_file_path(&pkg_name)
                                .ok()
                                .map(|p| p.parent().map(|p| p.to_path_buf()))
                                .unwrap_or(None);

                            if let Some(pkg_path) = pkg_path {
                                let task_node_file = task_node_block.entry_file();

                                if let Some(task_node_file) = task_node_file {
                                    let node_file = if flow_path.is_dir() {
                                        flow_path.join(task_node_file)
                                    } else {
                                        flow_path.parent().unwrap().join(task_node_file)
                                    };

                                    // update injection store for later use
                                    injection_nodes
                                        .entry(InjectionTarget::Package(pkg_path.clone()))
                                        .or_default()
                                        .insert(InjectionNode {
                                            node_id: task_node.node_id.clone(),
                                            absolute_entry: node_file,
                                            relative_entry: task_node_file.into(),
                                        });

                                    if let Some(script) = task_node
                                        .inject
                                        .as_ref()
                                        .and_then(|injection| injection.script.clone())
                                    {
                                        injection_scripts
                                            .entry(pkg_path.clone())
                                            .or_default()
                                            .push(script);
                                    }
                                }
                                BlockScope::Package {
                                    name: pkg_name,
                                    path: pkg_path,
                                    inject: true,
                                    node_id: None,
                                }
                            } else {
                                warn!("package not found: {:?}", pkg_name);
                                // maybe just throw error and exit will be better
                                BlockScope::default()
                            }
                        }
                    };

                    if !layer::feature_enabled() && running_scope.package_path().is_some() {
                        tracing::warn!(
                            "layer feature is not enabled, but task node {} has package path: {:?}. fallback to default scope",
                            task_node.node_id,
                            running_scope.package_path()
                        );
                        running_scope = BlockScope::default();
                    }

                    let merged_inputs_def =
                        if task.additional_inputs && task_node.inputs_def.is_some() {
                            let mut inputs_def = task.inputs_def.clone().unwrap_or_default();
                            if let Some(node_addition_inputs) = task_node.inputs_def.as_ref() {
                                for input in node_addition_inputs.iter() {
                                    inputs_def.entry(input.handle.to_owned()).or_insert_with(
                                        || InputHandle {
                                            is_additional: true,
                                            ..input.clone()
                                        },
                                    );
                                }
                            }
                            Some(inputs_def)
                        } else {
                            task.inputs_def.clone()
                        };

                    let inputs_def = merged_inputs_def.clone();

                    let inputs_def_patch = get_inputs_def_patch(&task_node.inputs_from);

                    let from = connections.node_inputs_froms.remove(&task_node.node_id);

                    let merged_outputs_def =
                        if task.additional_outputs && task.outputs_def.is_some() {
                            let mut outputs_def = task.outputs_def.clone().unwrap_or_default();
                            if let Some(node_addition_outputs) = task_node.outputs_def.as_ref() {
                                for output in node_addition_outputs.iter() {
                                    outputs_def.entry(output.handle.to_owned()).or_insert_with(
                                        || OutputHandle {
                                            is_additional: true,
                                            ..output.clone()
                                        },
                                    );
                                }
                            }
                            Some(outputs_def)
                        } else {
                            task.outputs_def.clone()
                        };

                    let mut task_inner = (*task).clone();
                    task_inner.outputs_def = merged_outputs_def;
                    task_inner.inputs_def = merged_inputs_def;
                    // TODO: this behavior change task's outputs_def, this task is a new task.
                    //       maybe we should refactor this later.
                    let task = Arc::new(task_inner);

                    let inputs = generate_node_inputs(
                        &inputs_def,
                        &from,
                        &task_node.inputs_from,
                        &inputs_def_patch,
                        &task_node.node_id,
                    );

                    new_nodes.insert(
                        task_node.node_id.to_owned(),
                        Node::Task(TaskNode {
                            description: task_node.description.clone(),
                            to: connections.node_outputs_tos.remove(&task_node.node_id),
                            node_id: task_node.node_id.to_owned(),
                            timeout: task_node.timeout,
                            scope: running_scope,
                            task,
                            inputs,
                            concurrency: task_node.concurrency,
                        }),
                    );
                }
                manifest::Node::Slot(slot_node) => {
                    let slot = block_resolver.resolve_slot_node_block(slot_node.slot.to_owned())?;
                    let inputs_def = slot.as_ref().inputs_def.clone();
                    let inputs_def_patch = get_inputs_def_patch(&slot_node.inputs_from);

                    let from = connections.node_inputs_froms.remove(&slot_node.node_id);
                    let inputs = generate_node_inputs(
                        &inputs_def,
                        &from,
                        &slot_node.inputs_from,
                        &inputs_def_patch,
                        &slot_node.node_id,
                    );
                    new_nodes.insert(
                        slot_node.node_id.to_owned(),
                        Node::Slot(SlotNode {
                            description: slot_node.description.clone(),
                            to: connections.node_outputs_tos.remove(&slot_node.node_id),
                            node_id: slot_node.node_id.to_owned(),
                            timeout: slot_node.timeout,
                            slot,
                            inputs,
                            concurrency: slot_node.concurrency,
                        }),
                    );
                }
            }
        }

        // merge injection nodes and scripts' key
        let key1 = injection_nodes
            .keys()
            .cloned()
            .collect::<HashSet<InjectionTarget>>();
        let key2 = injection_scripts
            .keys()
            .map(|k| InjectionTarget::Package(k.clone()))
            .collect::<HashSet<InjectionTarget>>();

        let intersection = key1.union(&key2);

        let mut injection = InjectionStore::new();
        for key in intersection {
            let nodes = injection_nodes
                .remove(key)
                .unwrap_or_default()
                .into_iter()
                .collect();
            match key {
                InjectionTarget::Package(pkg) => {
                    let scripts = injection_scripts.remove(pkg).unwrap_or_default();
                    let pkg_file = find_package_file(pkg).unwrap_or_default();
                    let pkg_meta = read_package(pkg_file)?;
                    injection.insert(
                        key.clone(),
                        InjectionMeta {
                            nodes,
                            scripts: Some(scripts),
                            package_version: pkg_meta.version.unwrap_or_default(),
                        },
                    );
                }
            }
        }

        if !injection.is_empty() {
            tracing::info!("injection: {:?}", injection);
        }

        let mut serialized_node_outputs: Vec<(NodeId, HandleName)> = Vec::new();

        for (_, node) in new_nodes.iter() {
            for (_, input) in node.inputs() {
                if input.serialize_for_cache {
                    if let Some(ref from) = input.sources {
                        for source in from.iter() {
                            match source {
                                crate::node::HandleSource::FlowInput { .. } => {
                                    // TODO: Serialization of flow input sources is currently not supported.
                                }
                                crate::node::HandleSource::NodeOutput {
                                    node_id: from_node_id,
                                    output_handle,
                                } => {
                                    serialized_node_outputs
                                        .push((from_node_id.clone(), output_handle.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }

        for (node_id, output_handle) in serialized_node_outputs {
            if let Some(node) = new_nodes.get_mut(&node_id) {
                node.update_outputs_def_serializable(&output_handle);
            }
        }

        Ok(Self {
            description: description,
            nodes: new_nodes,
            inputs_def,
            outputs_def,
            path_str: flow_path.to_string_lossy().to_string(),
            path: flow_path.clone(),
            flow_inputs_tos: connections.flow_inputs_tos.restore(),
            flow_outputs_froms: connections.flow_outputs_froms.restore(),
            package_path: package_path(&flow_path).ok(),
            injection_store: if injection.is_empty() {
                None
            } else {
                Some(injection)
            },
        })
    }

    pub fn get_services(&self) -> HashSet<ServiceQueryResult> {
        let mut services = HashSet::new();

        self.nodes.iter().for_each(|node| {
            if let Node::Service(service) = node.1 {
                let service_block_path = service.block.dir();
                let entry = if let Some(executor) = &service.block.service_executor.as_ref() {
                    executor.entry.clone()
                } else {
                    None
                };
                let package = if let Some(package_path) = &service.block.package_path {
                    package_path
                        .to_str()
                        .map(|package_path| package_path.to_string())
                } else {
                    None
                };

                let is_global = if let Some(e) = service.block.service_executor.as_ref() {
                    e.is_global()
                } else {
                    false
                };

                services.insert(ServiceQueryResult {
                    entry,
                    service_hash: utils::calculate_short_hash(&service_block_path, 16),
                    dir: service_block_path,
                    package,
                    is_global,
                });
            }
        });

        services
    }
}

fn get_inputs_def_patch(
    node_inputs_from: &Option<Vec<manifest::NodeInputFrom>>,
) -> Option<HashMap<HandleName, Vec<InputDefPatch>>> {
    match node_inputs_from {
        Some(inputs_from) => {
            let mut inputs_def_patch = HashMap::new();
            for input in inputs_from {
                if let Some(schema_overrides) = &input.schema_overrides {
                    inputs_def_patch.insert(input.handle.to_owned(), schema_overrides.clone());
                }
            }

            if inputs_def_patch.is_empty() {
                None
            } else {
                Some(inputs_def_patch)
            }
        }
        None => None,
    }
}
