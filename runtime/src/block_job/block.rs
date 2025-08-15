use job::JobId;

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
