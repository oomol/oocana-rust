use std::{collections::HashMap, sync::Arc};

use job::{BlockJobStacks, JobId, RuntimeScope};
use mainframe::scheduler::{QueryBlockRequest, RunBlockRequest};
use manifest_meta::{
    read_flow_or_block, BlockResolver, HandleName, HandleTo, InputHandle, InputHandles, Node,
    NodeId, OutputHandle, OutputHandles, SubflowBlock, TaskBlock,
};
use manifest_reader::path_finder::{self, calculate_block_value_type, BlockValueType};
use tracing::warn;
use utils::output::OutputValue;

use crate::{
    block_job::{self, fulfill_nullable_and_default},
    shared::Shared,
};

pub enum RunBlockSuccessResponse {
    Flow {
        flow_block: Arc<SubflowBlock>,
        inputs: HashMap<HandleName, Arc<OutputValue>>,
        scope: RuntimeScope,
        request_stack: BlockJobStacks,
        job_id: JobId,
        node_id: NodeId,
    },
    Task {
        task_block: Arc<TaskBlock>,
        inputs: HashMap<HandleName, Arc<OutputValue>>,
        scope: RuntimeScope,
        request_stack: BlockJobStacks,
        job_id: JobId,
        node_id: NodeId,
    },
}

pub fn parse_run_block_request(
    request: &RunBlockRequest,
    block_resolver: &mut BlockResolver,
    flow_path_finder: &mut path_finder::BlockPathFinder,
    shared: Arc<Shared>,
    scope: RuntimeScope,
) -> Result<RunBlockSuccessResponse, String> {
    let RunBlockRequest {
        block,
        payload,
        stacks,
        strict,
        block_job_id,
        ..
    } = request;

    let result = read_flow_or_block(&block, block_resolver, flow_path_finder);

    if result.is_err() {
        let msg = format!("Failed to read block or subflow: {}", block);
        return Err(msg);
    }

    let result_block = result.unwrap();

    let mut block_stack = BlockJobStacks::new();
    for s in stacks.iter() {
        block_stack = block_stack.stack(s.flow_job_id.clone(), s.flow.clone(), s.node_id.clone());
    }

    let validate_fn = |inputs_def: &Option<InputHandles>,
                       inputs: &HashMap<HandleName, Arc<OutputValue>>|
     -> HashMap<HandleName, String> {
        if *strict {
            return block_job::validate_inputs(inputs_def, inputs);
        }
        HashMap::new()
    };

    let mut values = payload
        .as_object()
        .and_then(|obj| obj.get("inputs"))
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<String, serde_json::Value>>()
        })
        .unwrap_or_default();

    match result_block {
        manifest_meta::Block::Task(task_block) => {
            let additional_inputs_def: HashMap<HandleName, InputHandle> = payload
                .as_object()
                .and_then(|obj| obj.get("additional_inputs_def"))
                .and_then(|v| v.as_array())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|v| serde_json::from_value::<InputHandle>(v.clone()).ok())
                        .map(|input| {
                            (
                                input.handle.to_owned(),
                                InputHandle {
                                    remember: false,
                                    is_additional: true,
                                    ..input
                                },
                            )
                        })
                        .collect::<HashMap<HandleName, InputHandle>>()
                })
                .unwrap_or_default();

            let additional_outputs_def: HashMap<HandleName, OutputHandle> = payload
                .as_object()
                .and_then(|obj| obj.get("additional_outputs_def"))
                .and_then(|v| v.as_array())
                .map(|obj| {
                    obj.iter()
                        .filter_map(|v| serde_json::from_value::<OutputHandle>(v.clone()).ok())
                        .map(|output| {
                            (
                                output.handle.to_owned(),
                                OutputHandle {
                                    is_additional: true,
                                    ..output
                                },
                            )
                        })
                        .collect::<HashMap<HandleName, OutputHandle>>()
                })
                .unwrap_or_default();

            let mut task_inner = (*task_block).clone();
            task_inner.inputs_def = task_inner.inputs_def.map(|mut inputs_def| {
                inputs_def.extend(additional_inputs_def);
                inputs_def
            });

            task_inner.outputs_def = task_inner.outputs_def.map(|mut outputs_def| {
                outputs_def.extend(additional_outputs_def);
                outputs_def
            });

            let task_block = Arc::new(task_inner);

            fulfill_nullable_and_default(&mut values, &task_block.inputs_def);

            let inputs_values: HashMap<HandleName, Arc<OutputValue>> = values
                .into_iter()
                .map(|(handle, value)| {
                    (
                        HandleName::new(handle),
                        Arc::new(OutputValue {
                            value,
                            is_json_serializable: true,
                        }),
                    )
                })
                .collect();

            let missing_inputs = task_block
                .inputs_def
                .as_ref()
                .map(|inputs_def| {
                    inputs_def
                        .iter()
                        .filter_map(|(handle, _)| {
                            (!inputs_values.contains_key(handle)).then_some(handle.clone())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !missing_inputs.is_empty() {
                let msg = format!(
                    "Task block {} inputs missing these input handles: {:?}",
                    block, missing_inputs
                );
                return Err(msg);
            }

            let invalid_inputs = validate_fn(&task_block.inputs_def, &inputs_values);

            if !invalid_inputs.is_empty() {
                let mut msg = format!("Task block {} has some invalid inputs:", block);
                for (handle, error) in invalid_inputs {
                    msg += format!("\n{}: {}", handle, error).as_str();
                }
                return Err(msg);
            }

            let block_scope = match calculate_block_value_type(&block) {
                BlockValueType::Pkg { pkg_name, .. } => {
                    RuntimeScope {
                        session_id: shared.session_id.clone(),
                        pkg_name: Some(pkg_name.clone()),
                        data_dir: scope.pkg_root.join(pkg_name).to_string_lossy().to_string(),
                        pkg_root: scope.pkg_root.clone(),
                        path: task_block.package_path.clone().unwrap_or_else(|| {
                            // if package path is not set, use flow shared scope package path
                            warn!("can not find block package path, this should never happen");
                            scope.path.clone()
                        }),
                        node_id: None,
                        is_inject: false,
                        enable_layer: layer::feature_enabled(),
                    }
                }
                _ => scope.clone(),
            };

            return Ok(RunBlockSuccessResponse::Task {
                task_block,
                inputs: inputs_values,
                scope: block_scope,
                job_id: block_job_id.to_owned().into(),
                request_stack: block_stack,
                node_id: NodeId::from(format!("run_block::{}", request.block)),
            });
        }
        manifest_meta::Block::Flow(subflow_block) => {
            let flow_scope = match calculate_block_value_type(&block) {
                BlockValueType::Pkg { pkg_name, .. } => RuntimeScope {
                    session_id: shared.session_id.clone(),
                    pkg_name: Some(pkg_name.clone()),
                    data_dir: scope.pkg_root.join(pkg_name).to_string_lossy().to_string(),
                    pkg_root: scope.pkg_root.clone(),
                    path: subflow_block.package_path.clone().unwrap_or_else(|| {
                        warn!("can not find subflow package path, this should never happen");
                        scope.path.clone()
                    }),
                    node_id: None,
                    is_inject: false,
                    enable_layer: layer::feature_enabled(),
                },
                _ => scope.clone(),
            };

            fulfill_nullable_and_default(&mut values, &subflow_block.inputs_def);

            let input_values: HashMap<HandleName, Arc<OutputValue>> = values
                .into_iter()
                .map(|(handle, value)| {
                    (
                        HandleName::new(handle),
                        Arc::new(OutputValue {
                            value,
                            is_json_serializable: true,
                        }),
                    )
                })
                .collect();

            let missing_inputs = subflow_block
                .inputs_def
                .as_ref()
                .map(|inputs_def| {
                    inputs_def
                        .iter()
                        .filter_map(|(handle, _)| {
                            (!input_values.contains_key(handle)).then_some(handle.clone())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !missing_inputs.is_empty() {
                let msg = format!(
                    "Subflow block {} inputs missing these input handles: {:?}",
                    block, missing_inputs
                );
                return Err(msg);
            }

            let invalid_inputs = validate_fn(&subflow_block.inputs_def, &input_values);

            if !invalid_inputs.is_empty() {
                let mut msg = format!("Subflow block {} has some invalid inputs:", block);
                for (handle, error) in invalid_inputs {
                    msg += format!("\n{}: {}", handle, error).as_str();
                }
                return Err(msg);
            }

            return Ok(RunBlockSuccessResponse::Flow {
                flow_block: subflow_block,
                inputs: input_values,
                scope: flow_scope,
                job_id: block_job_id.to_owned().into(),
                request_stack: block_stack,
                node_id: NodeId::from(format!("run_block::{}", request.block)),
            });
        }
        _ => {
            let msg = format!("{} is not subflow or task block.", block);
            return Err(msg);
        }
    }
}

pub fn parse_query_block_request(
    request: &QueryBlockRequest,
    block_resolver: &mut BlockResolver,
    flow_path_finder: &mut path_finder::BlockPathFinder,
) -> Result<serde_json::Value, String> {
    let result = read_flow_or_block(&request.block, block_resolver, flow_path_finder);

    match result {
        Ok(block) => match block {
            manifest_meta::Block::Task(task_block) => {
                #[derive(serde::Serialize)]
                struct TaskBlockMetadata {
                    pub r#type: String,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub description: Option<String>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub inputs_def: Option<InputHandles>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    pub outputs_def: Option<OutputHandles>,
                    pub additional_inputs: bool,
                    pub additional_outputs: bool,
                }

                let metadata = TaskBlockMetadata {
                    r#type: "task".to_string(),
                    description: task_block.description.clone(),
                    inputs_def: task_block.inputs_def.clone(),
                    outputs_def: task_block.outputs_def.clone(),
                    additional_inputs: task_block.additional_inputs,
                    additional_outputs: task_block.additional_outputs,
                };
                let json = serde_json::to_value(&metadata);
                if let Ok(json) = json {
                    return Ok(json);
                } else {
                    return Err(format!("Failed to serialize task block metadata to JSON"));
                }
            }
            manifest_meta::Block::Flow(subflow_block) => {
                #[derive(serde::Serialize)]
                struct SubflowMetadata {
                    r#type: String,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    description: Option<String>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    inputs_def: Option<InputHandles>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    outputs_def: Option<OutputHandles>,
                    has_slot: bool,
                }

                let metadata = SubflowMetadata {
                    r#type: "subflow".to_string(),
                    description: subflow_block.description.clone(),
                    inputs_def: subflow_block.inputs_def.clone(),
                    outputs_def: subflow_block.outputs_def.clone(),
                    has_slot: subflow_block.has_slot(),
                };

                let json = serde_json::to_value(&metadata);
                if let Ok(json) = json {
                    return Ok(json);
                } else {
                    return Err(format!(
                        "Failed to serialize subflow block metadata to JSON"
                    ));
                }
            }
            _ => {
                return Err(format!("{} is not subflow or task block.", request.block));
            }
        },
        Err(_) => {
            return Err(format!(
                "Failed to find {} for block or subflow",
                request.block
            ));
        }
    }
}

pub fn parse_node_downstream(
    query_node: Option<&Node>,
    nodes: &HashMap<NodeId, Node>,
    query_output_handles: &Option<Vec<HandleName>>,
    outputs_def: &Option<OutputHandles>,
) -> Result<serde_json::Value, String> {
    let query_node = match query_node {
        Some(node) => node,
        None => {
            // if none return empty downstream
            return serde_json::to_value(HashMap::<HandleName, ()>::new())
                .map_err(|e| format!("Failed to serialize empty downstream: {}", e));
        }
    };

    #[derive(serde::Serialize)]
    struct FlowDownstream {
        output_handle: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_handle_def: Option<OutputHandle>,
    }

    #[derive(serde::Serialize)]
    struct NodeDownstream {
        node_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        input_handle: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_handle_def: Option<InputHandle>,
    }

    #[derive(serde::Serialize, Default)]
    struct Downstream {
        #[serde(skip_serializing_if = "Option::is_none")]
        to_flow: Option<Vec<FlowDownstream>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        to_node: Option<Vec<NodeDownstream>>,
    }

    let mut downstream: HashMap<HandleName, Downstream> = HashMap::new();
    if let Some(tos) = query_node.to() {
        for (handle, tos) in tos {
            if query_output_handles
                .as_ref()
                .is_none_or(|o| o.contains(handle))
            {
                for to in tos {
                    match to {
                        HandleTo::ToFlowOutput { output_handle, .. } => {
                            let flow_downstream = downstream
                                .entry(handle.clone())
                                .or_default()
                                .to_flow
                                .get_or_insert_with(Vec::new);
                            flow_downstream.push(FlowDownstream {
                                output_handle: output_handle.to_string(),
                                output_handle_def: outputs_def
                                    .as_ref()
                                    .and_then(|def| def.get(output_handle))
                                    .cloned(),
                            });
                        }
                        HandleTo::ToNodeInput {
                            node_id,
                            input_handle,
                            ..
                        } => {
                            let node_downstream = downstream
                                .entry(handle.clone())
                                .or_default()
                                .to_node
                                .get_or_insert_with(Vec::new);
                            node_downstream.push(NodeDownstream {
                                node_id: node_id.to_string(),
                                description: nodes.get(node_id).and_then(|n| n.description()),
                                input_handle: input_handle.to_string(),
                                input_handle_def: nodes
                                    .get(node_id)
                                    .and_then(|n| n.inputs_def())
                                    .as_ref()
                                    .and_then(|inputs_def| inputs_def.get(input_handle))
                                    .cloned(),
                            });
                        }
                    }
                }
            }
        }
    }

    let res = serde_json::to_value(downstream);
    match res {
        Ok(value) => Ok(value),
        Err(e) => Err(format!("Failed to serialize downstream: {}", e)),
    }
}
