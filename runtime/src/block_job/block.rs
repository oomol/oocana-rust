use std::{collections::HashSet, sync::Arc};

use job::JobId;
use manifest_meta::{NodeId, SubflowBlock};

use crate::flow_job::{self};

pub struct BlockJobHandle {
    // TODO: Remove this field
    #[allow(dead_code)]
    pub job_id: JobId,
    _job: Box<dyn Send>,
}

impl BlockJobHandle {
    // TODO: Remove this field
    #[allow(dead_code)]
    pub fn get_job_id(&self) -> &JobId {
        &self.job_id
    }
}

impl BlockJobHandle {
    pub fn new(job_id: JobId, job: impl Send + 'static) -> Self {
        Self {
            job_id,
            _job: Box::new(job),
        }
    }
}

pub struct FindUpstreamArgs {
    pub flow_block: Arc<SubflowBlock>,
    pub use_cache: bool,
    pub nodes: Option<HashSet<NodeId>>,
}

pub fn find_upstream(args: FindUpstreamArgs) -> (Vec<String>, Vec<String>, Vec<String>) {
    let FindUpstreamArgs {
        flow_block,
        nodes,
        use_cache,
    } = args;
    let upstream_args = flow_job::UpstreamArgs {
        flow_block,
        use_cache,
        nodes,
    };
    flow_job::find_upstream(upstream_args)
}
