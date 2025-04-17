use std::{
    collections::{HashMap, HashSet},
    fmt,
    fmt::Formatter,
    path::PathBuf,
};

use manifest_reader::{
    manifest::{self, HandleName, InputDefPatch, InputHandles, OutputHandles},
    path_finder::{find_package_file, BlockPathFinder},
    reader::read_package,
};

use crate::scope::{calculate_running_scope, RunningScope, RunningTarget};

use tracing::warn;
use utils::error::Result;

use crate::{
    block_resolver::{package_path, BlockResolver},
    connections::Connections,
    node::ServiceNode,
    HandleFrom, HandlesFroms, HandlesTos, Node, NodeId, SlotNode, SubflowNode, TaskNode,
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

impl SubflowBlock {
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

        let mut connections =
            Connections::new(nodes.iter().map(|n| n.node_id().to_owned()).collect());

        connections.parse_flow_outputs_from(outputs_from);

        for node in nodes.iter() {
            connections.parse_node_inputs_from(node.node_id(), node.inputs_from());
            if let manifest::Node::Subflow(subflow_node) = node {
                connections.parse_subflow_slot_outputs_from(
                    &subflow_node.node_id,
                    subflow_node.slots.as_ref(),
                );
            }
        }

        let value_nodes = nodes
            .iter()
            .filter_map(|node| match node {
                manifest::Node::Value(value_node) => Some(value_node.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let value_nodes_id = value_nodes
            .iter()
            .map(|n| n.node_id.clone())
            .collect::<Vec<_>>();

        // node 的 skip 属性为 true，这个 node 不会放到 flow 列表，但是仍然保留连线逻辑。所以要在 parse_node_inputs_from 处理完之后删除。
        let nodes_in_flow: Vec<manifest::Node> = nodes
            .into_iter()
            .filter(|node| !node.should_ignore() && !value_nodes_id.contains(node.node_id()))
            .collect();

        let find_node = |node_id: &NodeId| -> Option<&manifest::Node> {
            nodes_in_flow.iter().find(|n| n.node_id() == node_id)
        };

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
                    let flow = block_resolver
                        .resolve_flow_block(&subflow_node.subflow, &mut path_finder)?;
                    let inputs_def =
                        parse_inputs_def(&subflow_node.inputs_from, &flow.as_ref().inputs_def);
                    let inputs_def_patch = get_inputs_def_patch(&subflow_node.inputs_from);

                    // TODO: flow 注入

                    new_nodes.insert(
                        subflow_node.node_id.to_owned(),
                        Node::Flow(SubflowNode {
                            from: connections.node_inputs_froms.remove(&subflow_node.node_id),
                            to: connections.node_outputs_tos.remove(&subflow_node.node_id),
                            slots_outputs_from: connections
                                .slot_outputs_froms
                                .remove(&subflow_node.node_id),
                            slots_inputs_to: connections
                                .slot_inputs_tos
                                .remove(&subflow_node.node_id),
                            flow,
                            node_id: subflow_node.node_id.to_owned(),
                            timeout: subflow_node.timeout,
                            inputs_def,
                            concurrency: subflow_node.concurrency,
                            inputs_def_patch,
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
                    let running_target = calculate_running_scope(
                        node,
                        &task_node.inject,
                        &task.package_path,
                        task_node_block.block_type(),
                    );

                    // if flow_paths s [/a/b/c]/flows/AAA/flow.oo.yaml return [/a/b/c]
                    // else return flow_paths's parent dir
                    let grandparent_dir = flow_path.parent().and_then(|p| p.parent());
                    let workspace = if grandparent_dir
                        .is_some_and(|p| p.exists() && p.file_name() == Some("flows".as_ref()))
                    {
                        grandparent_dir.and_then(|p| p.parent())
                    } else {
                        flow_path.parent()
                    };

                    let mut running_scope = match running_target {
                        RunningTarget::Global => RunningScope::Global {
                            node_id: None,
                            workspace: workspace.map(|p| p.to_path_buf()),
                        },
                        RunningTarget::PackagePath { path, node_id } => RunningScope::Package {
                            name: None,
                            path,
                            node_id,
                        },
                        RunningTarget::Node(node_id) => match find_node(&node_id) {
                            Some(_) => RunningScope::Global {
                                node_id: Some(node_id),
                                workspace: workspace.map(|p| p.to_path_buf()),
                            },
                            None => {
                                warn!("target node not found: {:?}", node_id);
                                RunningScope::Global {
                                    node_id: None,
                                    workspace: workspace.map(|p| p.to_path_buf()),
                                }
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
                                        .and_then(|injection| injection.script.clone()) { injection_scripts
                                                .entry(pkg_path.clone())
                                                .or_default()
                                                .push(script); }
                                }
                                RunningScope::Package {
                                    name: Some(name),
                                    path: pkg_path,
                                    node_id: None,
                                }
                            } else {
                                warn!("package not found: {:?}", name);
                                // maybe just throw error and exit will be better
                                RunningScope::Global {
                                    node_id: None,
                                    workspace: workspace.map(|p| p.to_path_buf()),
                                }
                            }
                        }
                    };

                    if !layer::feature_enabled() && running_scope.package_path().is_some() {
                        running_scope = RunningScope::Global {
                            node_id: None,
                            workspace: workspace.map(|p| p.to_path_buf()),
                        };
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

                    let mut inputs_def =
                        parse_inputs_def(&task_node.inputs_from, &merge_inputs_def);

                    let inputs_def_patch = get_inputs_def_patch(&task_node.inputs_from);

                    let mut froms: Option<HashMap<HandleName, Vec<HandleFrom>>> =
                        connections.node_inputs_froms.remove(&task_node.node_id);
                    let mut has_value_node = false;

                    // TODO: value node 应该支持其他的几种 node，暂时没实现。
                    if let Some(ref froms) = froms {
                        for (handle_name, froms) in froms {
                            for from in froms {
                                if let HandleFrom::FromNodeOutput {
                                    node_id,
                                    node_output_handle,
                                } = from
                                {
                                    if let Some(value_node) =
                                        value_nodes.iter().find(|n| &n.node_id == node_id)
                                    {
                                        has_value_node = true;
                                        if let Some(handle) = value_node
                                            .values
                                            .iter()
                                            .find(|v| v.handle == *node_output_handle)
                                        {
                                            if let Some(ref mut input_defs) = inputs_def {
                                                if value_node.ignore {
                                                    input_defs
                                                        .entry(handle_name.to_owned())
                                                        .and_modify(|def| def.value = None);
                                                } else {
                                                    input_defs
                                                        .entry(handle_name.to_owned())
                                                        .and_modify(|def| {
                                                            def.value = handle.value.clone()
                                                        });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if has_value_node {
                        // value node 不会运行，所以不能作为 from to 的一部分
                        if let Some(ref mut froms) = froms {
                            for (_, froms) in froms.iter_mut() {
                                froms.retain(|from| {
                                    if let HandleFrom::FromNodeOutput { node_id, .. } = from {
                                        !value_nodes_id.contains(node_id)
                                    } else {
                                        true
                                    }
                                });
                            }
                        }
                    }

                    new_nodes.insert(
                        task_node.node_id.to_owned(),
                        Node::Task(TaskNode {
                            from: froms,
                            to: connections.node_outputs_tos.remove(&task_node.node_id),
                            node_id: task_node.node_id.to_owned(),
                            timeout: task_node.timeout,
                            scope: running_scope,
                            task,
                            inputs_def,
                            concurrency: task_node.concurrency,
                            timeout_seconds: task_node.timeout_seconds,
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
                            // title: subflow_node.title,
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

        self.nodes.iter().for_each(|node| if let Node::Service(service) = node.1 {
            let service_block_path = service.block.dir();
            let entry = if let Some(executor) = &service.block.service_executor.as_ref() {
                executor.entry.clone()
            } else {
                None
            };
            let package = if let Some(package_path) = &service.block.package_path {
                package_path.to_str().map(|package_path| package_path.to_string())
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
