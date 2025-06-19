use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Formatter},
    path::PathBuf,
    sync::Arc,
};

use manifest_reader::{
    manifest::{self, HandleName, InputDefPatch, InputHandle, InputHandles, OutputHandles},
    path_finder::{calculate_block_value_type, find_package_file, BlockPathFinder, BlockValueType},
    reader::read_package,
};

use crate::{
    node::subflow::{Slot, SubflowSlot, TaskSlot},
    scope::{calculate_running_target, RunningScope, RunningTarget},
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

impl SubflowBlock {
    pub fn has_slot(&self) -> bool {
        self.nodes
            .values()
            .any(|node| matches!(node, Node::Slot(_)))
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
                    subflow_node.slots.as_ref().map(|slots| {
                        for provider in slots {
                            connections.parse_slot_inputs_from(
                                node.node_id(),
                                &provider.node_id(),
                                provider.inputs_from(),
                                &find_value_node,
                            );
                        }
                    });
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
                    let subflow_inputs_def =
                        parse_inputs_def(&subflow_node.inputs_from, &flow.as_ref().inputs_def);
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
                        RunningTarget::Inherit => RunningScope::default(),
                        RunningTarget::PackagePath { path, node_id } => RunningScope::Package {
                            name: None,
                            path,
                            node_id,
                        },
                        _ => {
                            warn!("subflow node injection not supported");
                            RunningScope::default()
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

                                    let scope = if matches!(
                                        calculate_block_value_type(&task_slot_provider.task),
                                        BlockValueType::Pkg { .. }
                                    ) && task.package_path.is_some()
                                    {
                                        RunningScope::Package {
                                            path: task.package_path.clone().unwrap(),
                                            name: None,
                                            node_id: None,
                                        }
                                    } else {
                                        RunningScope::Slot {}
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
                                    let slotflow = block_resolver.resolve_slot_flow_block(
                                        &slotflow_provider.slotflow,
                                        &mut path_finder,
                                    )?;

                                    let scope = if matches!(
                                        calculate_block_value_type(&slotflow_provider.slotflow),
                                        BlockValueType::Pkg { .. }
                                    ) && slotflow.package_path.is_some()
                                    {
                                        RunningScope::Package {
                                            path: slotflow.package_path.clone().unwrap(),
                                            name: None,
                                            node_id: None,
                                        }
                                    } else {
                                        RunningScope::Slot {}
                                    };

                                    let slot_block = Slot::Subflow(SubflowSlot {
                                        slot_node_id: slotflow_provider.slot_node_id.to_owned(),
                                        subflow: slotflow,
                                        scope,
                                    });

                                    if let Some(slot_node) =
                                        flow.nodes.get(&slotflow_provider.slot_node_id)
                                    {
                                        if let Node::Slot(slot_node) = slot_node {
                                            let mut new_inputs_def = slot_node
                                                .slot
                                                .as_ref()
                                                .inputs_def
                                                .clone()
                                                .unwrap_or_default();

                                            let mut new_froms =
                                                slot_node.from.clone().unwrap_or_default();
                                            let mut addition_flow_inputs_tos: HashMap<
                                                HandleName,
                                                Vec<crate::HandleTo>,
                                            > = HashMap::new();
                                            if let Some(addition_def) =
                                                &slotflow_provider.inputs_def
                                            {
                                                for input in addition_def.iter() {
                                                    let runtime_handle_name =
                                                        generate_runtime_handle_name(
                                                            &slot_node.node_id,
                                                            &input.handle,
                                                        );
                                                    if !new_inputs_def.contains_key(&input.handle) {
                                                        // add slot node inputs_def
                                                        new_inputs_def.insert(
                                                            input.handle.to_owned(),
                                                            // this input should be always remembered true
                                                            InputHandle {
                                                                remember: true,
                                                                ..input.clone()
                                                            },
                                                        );

                                                        // add subflow inputs_def
                                                        addition_subflow_inputs_def.insert(
                                                            runtime_handle_name.clone(),
                                                            InputHandle {
                                                                handle: runtime_handle_name.clone(),
                                                                ..input.clone()
                                                            },
                                                        );
                                                    } else {
                                                        tracing::warn!(
                                                            "slot node {} already has input {}",
                                                            slotflow_provider.slot_node_id,
                                                            input.handle
                                                        );
                                                        continue;
                                                    }

                                                    if let Some(input_from) = slotflow_provider
                                                        .inputs_from
                                                        .as_ref()
                                                        .and_then(|inputs_from| {
                                                            inputs_from
                                                                .iter()
                                                                .find(|i| i.handle == input.handle)
                                                        })
                                                    {
                                                        new_froms
                                                            .entry(input.handle.to_owned())
                                                            .or_default()
                                                            .push(
                                                                crate::HandleFrom::FromFlowInput {
                                                                    input_handle:
                                                                        runtime_handle_name.clone(),
                                                                },
                                                            );

                                                        if let Some(value) = &input_from.value {
                                                            addition_subflow_inputs_def
                                                                .entry(runtime_handle_name.clone())
                                                                .and_modify(|def| {
                                                                    def.value = Some(value.clone());
                                                                });
                                                        }
                                                    } else {
                                                        warn!(
                                                            "slot node {} input {} not found in inputs_from",
                                                            slotflow_provider.slot_node_id,
                                                            input.handle
                                                        );
                                                    }

                                                    addition_flow_inputs_tos
                                                        .entry(runtime_handle_name.to_owned())
                                                        .or_default()
                                                        .push(crate::HandleTo::ToNodeInput {
                                                            node_id: slot_node.node_id.clone(),
                                                            node_input_handle: input
                                                                .handle
                                                                .to_owned(),
                                                        });
                                                }

                                                let mut new_slot_node = slot_node.clone();
                                                {
                                                    new_slot_node.inputs_def = Some(new_inputs_def);
                                                    new_slot_node.from = Some(new_froms);
                                                }

                                                // Cannot mutate inside Arc, so clone, update, and re-wrap if needed
                                                let mut flow_inner = (*flow).clone();
                                                flow_inner.update_node(
                                                    &slotflow_provider.slot_node_id,
                                                    Node::Slot(new_slot_node),
                                                );
                                                for (input_handle, handle_tos) in
                                                    addition_flow_inputs_tos.iter()
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
                                                "{} is not a slot node",
                                                slotflow_provider.slot_node_id
                                            );
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

                                    let scope = if matches!(
                                        calculate_block_value_type(&subflow_provider.subflow),
                                        BlockValueType::Pkg { .. }
                                    ) && slot_flow.package_path.is_some()
                                    {
                                        RunningScope::Package {
                                            path: slot_flow.package_path.clone().unwrap(),
                                            name: None,
                                            node_id: None,
                                        }
                                    } else {
                                        RunningScope::Slot {}
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
                                        remember: true,
                                        ..input.clone()
                                    },
                                );
                            }
                        }
                        Some(merged_inputs_def)
                    } else {
                        subflow_inputs_def
                    };

                    new_nodes.insert(
                        subflow_node.node_id.to_owned(),
                        Node::Flow(SubflowNode {
                            from: connections.node_inputs_froms.remove(&subflow_node.node_id),
                            to,
                            flow,
                            node_id: subflow_node.node_id.to_owned(),
                            timeout: subflow_node.timeout,
                            inputs_def,
                            concurrency: subflow_node.concurrency,
                            inputs_def_patch: subflow_inputs_def_patch,
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
                    let inputs_def =
                        parse_inputs_def(&service_node.inputs_from, &service.as_ref().inputs_def);
                    let inputs_def_patch = get_inputs_def_patch(&service_node.inputs_from);

                    new_nodes.insert(
                        service_node.node_id.to_owned(),
                        Node::Service(ServiceNode {
                            from: connections.node_inputs_froms.remove(&service_node.node_id),
                            to: connections.node_outputs_tos.remove(&service_node.node_id),
                            node_id: service_node.node_id.to_owned(),
                            // title: subflow_node.title,
                            timeout: service_node.timeout,
                            block: service,
                            inputs_def,
                            concurrency: service_node.concurrency,
                            inputs_def_patch,
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
                        RunningTarget::Inherit => RunningScope::default(),
                        RunningTarget::PackagePath { path, node_id } => RunningScope::Package {
                            name: None,
                            path,
                            node_id,
                        },
                        RunningTarget::Node(node_id) => match find_node(&node_id) {
                            Some(_) => RunningScope::Flow {
                                node_id: Some(node_id),
                            },
                            None => {
                                warn!("target node not found: {:?}", node_id);
                                RunningScope::default()
                            }
                        },
                        RunningTarget::PackageName(name) => {
                            let pkg_path = path_finder
                                .find_package_file_path(&name)
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
                                RunningScope::Package {
                                    name: Some(name),
                                    path: pkg_path,
                                    node_id: None,
                                }
                            } else {
                                warn!("package not found: {:?}", name);
                                // maybe just throw error and exit will be better
                                RunningScope::default()
                            }
                        }
                    };

                    if !layer::feature_enabled() && running_scope.package_path().is_some() {
                        running_scope = RunningScope::default();
                    }

                    let merge_inputs_def =
                        if task.additional_inputs && task_node.inputs_def.is_some() {
                            let mut inputs_def = task.inputs_def.clone().unwrap_or_default();
                            if let Some(node_addition_inputs) = task_node.inputs_def.as_ref() {
                                for input in node_addition_inputs.iter() {
                                    if !inputs_def.contains_key(&input.handle) {
                                        inputs_def.insert(input.handle.to_owned(), input.clone());
                                    }
                                }
                            }
                            Some(inputs_def)
                        } else {
                            task.inputs_def.clone()
                        };

                    let inputs_def = parse_inputs_def(&task_node.inputs_from, &merge_inputs_def);

                    let inputs_def_patch = get_inputs_def_patch(&task_node.inputs_from);

                    let from_map = connections.node_inputs_froms.remove(&task_node.node_id);

                    new_nodes.insert(
                        task_node.node_id.to_owned(),
                        Node::Task(TaskNode {
                            from: from_map,
                            to: connections.node_outputs_tos.remove(&task_node.node_id),
                            node_id: task_node.node_id.to_owned(),
                            timeout: task_node.timeout,
                            scope: running_scope,
                            task,
                            inputs_def,
                            concurrency: task_node.concurrency,
                            inputs_def_patch,
                        }),
                    );
                }
                manifest::Node::Slot(slot_node) => {
                    let slot = block_resolver.resolve_slot_node_block(slot_node.slot.to_owned())?;
                    let inputs_def =
                        parse_inputs_def(&slot_node.inputs_from, &slot.as_ref().inputs_def);
                    let inputs_def_patch = get_inputs_def_patch(&slot_node.inputs_from);

                    new_nodes.insert(
                        slot_node.node_id.to_owned(),
                        Node::Slot(SlotNode {
                            from: connections.node_inputs_froms.remove(&slot_node.node_id),
                            to: connections.node_outputs_tos.remove(&slot_node.node_id),
                            node_id: slot_node.node_id.to_owned(),
                            timeout: slot_node.timeout,
                            slot,
                            inputs_def,
                            concurrency: slot_node.concurrency,
                            inputs_def_patch,
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

        Ok(Self {
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

fn parse_inputs_def(
    node_inputs_from: &Option<Vec<manifest::NodeInputFrom>>,
    inputs_def: &Option<InputHandles>,
) -> Option<InputHandles> {
    match inputs_def {
        Some(inputs_def) => {
            let mut merged_inputs_def = inputs_def.clone();
            if let Some(inputs_from) = node_inputs_from {
                for input in inputs_from {
                    // 如果 node_inputs_from 中的 input 不在 inputs_def 中，则不合并进 node 实例中的 inputs_def 中。这样可以避免后续做很多判断。
                    if !merged_inputs_def.contains_key(&input.handle) {
                        continue;
                    }

                    merged_inputs_def
                        .entry(input.handle.to_owned())
                        .and_modify(|def| {
                            // input_def 里面的 value 在实际运行中，是不会被使用的，要用 from 里面的值替换。
                            // 如果 from 里面没有对应 input def 的值，也需要考虑删除 def 的值。目前没考虑。
                            def.value = input.value.clone();
                        });
                }
            }

            Some(merged_inputs_def)
        }
        None => None,
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
