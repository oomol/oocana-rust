use std::{collections::HashMap, sync::Arc};

use job::{BlockJobStacks, JobId, RuntimeScope};
use mainframe::scheduler::RunBlockRequest;
use manifest_meta::{
    read_flow_or_block, BlockResolver, HandleName, InputHandle, InputHandles, NodeId, OutputHandle,
    SubflowBlock, TaskBlock,
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
        if !strict.unwrap_or(false) {
            return HashMap::new();
        }
        block_job::validate_inputs(inputs_def, inputs)
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
                let mut msg = "run block api has some invalid inputs:".to_string();
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
                            warn!("can find block package path, this should never happen");
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
                        warn!("can find subflow package path, this should never happen");
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
                    "subflow block {} inputs missing these input handles: {:?}",
                    block, missing_inputs
                );
                return Err(msg);
            }

            let invalid_inputs = validate_fn(&subflow_block.inputs_def, &input_values);

            if !invalid_inputs.is_empty() {
                let mut msg = "run block api has some invalid inputs:".to_string();
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
            let msg = format!("block not found for run block request: {}", block);
            return Err(msg);
        }
    }
}
