pub struct BlockJobHandle {
    _job: Box<dyn Send>,
}

impl BlockJobHandle {
    pub fn new(job: impl Send + 'static) -> Self {
        Self {
            _job: Box::new(job),
        }
    }
}
