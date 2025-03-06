use crate::sink::{BlockTaskSink, PipelineTask};
use std::collections::HashMap;

/// A pipeline task sink contains all the pipeline tasks and its associate block task sinks.

pub struct PipelineTaskSink {
    pipeline_tasks: HashMap<String, PipelineTask>,
    block_task_sinks: HashMap<String, BlockTaskSink>,
}

impl PipelineTaskSink {
    pub fn new() -> Self {
        PipelineTaskSink {
            pipeline_tasks: HashMap::new(),
            block_task_sinks: HashMap::new(),
        }
    }

    /// Returns `true` if the the specific pipeline task exists.
    pub fn contains(&self, pipeline_task_id: &str) -> bool {
        self.pipeline_tasks.contains_key(pipeline_task_id)
    }

    pub fn get_pipeline_task(&self, pipeline_task_id: &str) -> Option<&PipelineTask> {
        self.pipeline_tasks.get(pipeline_task_id)
    }

    pub fn get_block_task_sink(&self, pipeline_task_id: &str) -> Option<&BlockTaskSink> {
        self.block_task_sinks.get(pipeline_task_id)
    }

    /// Create a pipeline task from a pipeline.
    pub fn create(&mut self, pipeline_id: &str) {
        let pipeline_task = PipelineTask::new(pipeline_id);
        self.block_task_sinks.insert(
            pipeline_task.task_id.to_owned(),
            BlockTaskSink::new(&pipeline_task.task_id),
        );
        self.pipeline_tasks
            .insert(pipeline_task.task_id.to_owned(), pipeline_task);
    }

    /// Mark a pipeline task as success.
    pub fn done(&mut self, pipeline_task_id: &str) {
        if let Some(pipeline_task) = self.pipeline_tasks.get_mut(pipeline_task_id) {
            pipeline_task.done();
        }
    }

    /// Removes a pipeline task and its associate block task sink, returning the pipeline task at the id if it was previously exists.
    pub fn remove(&mut self, pipeline_task_id: &str) -> Option<PipelineTask> {
        self.block_task_sinks.remove(pipeline_task_id);
        self.pipeline_tasks.remove(pipeline_task_id)
    }
}
