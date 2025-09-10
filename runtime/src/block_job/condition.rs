use std::{collections::HashMap, sync::Arc};

use job::{BlockInputs, BlockJobStacks, JobId, RuntimeScope};
use manifest_meta::ConditionBlock;

use crate::{block_status::BlockStatusTx, shared::Shared};

use super::BlockJobHandle;

pub struct ConditionJobParameters {
    pub condition_block: Arc<ConditionBlock>,
    pub shared: Arc<Shared>,
    pub stacks: BlockJobStacks,
    pub job_id: JobId,
    pub inputs: Option<BlockInputs>,
    pub block_status: BlockStatusTx,
    #[allow(dead_code)]
    pub scope: RuntimeScope,
    pub output_def: Option<manifest_meta::OutputHandle>,
}

pub struct ConditionJobHandle {}

pub fn execute_condition_job(params: ConditionJobParameters) -> Option<BlockJobHandle> {
    let ConditionJobParameters {
        condition_block,
        shared,
        stacks,
        job_id,
        inputs,
        block_status,
        scope: _,
        output_def,
    } = params;

    let reporter = Arc::new(shared.reporter.block(job_id.to_owned(), None, stacks));
    reporter.started(&inputs);

    let inputs_values = if let Some(inputs) = inputs.clone() {
        let mut map = HashMap::new();
        for (k, v) in inputs.iter() {
            map.insert(k.clone(), v.value.clone());
        }
        map
    } else {
        HashMap::new()
    };

    let output_handle = condition_block.evaluate(&inputs_values);
    if output_handle.is_none() {
        block_status.finish(job_id.clone(), None, None);
        reporter.finished(None, None);
        return None;
    }
    let output_handle = output_handle.unwrap();

    let output_value = inputs.as_ref().and_then(|inputs| {
        let handle = output_def.map(|o| o.handle);
        if let Some(handle) = handle {
            inputs.get(&handle).cloned()
        } else {
            None
        }
    });

    if let Some(output) = output_value {
        let result = HashMap::from([(output_handle, output)]);
        block_status.finish(job_id.clone(), Some(result.clone()), None);

        let result_map = result
            .iter()
            .map(|(k, v)| (k.to_string(), v.value.clone()))
            .collect();
        reporter.finished(Some(result_map), None);
    }

    Some(BlockJobHandle::new(
        job_id.to_owned(),
        ConditionJobHandle {},
    ))
}
