use crate::sink::BlockTask;
use std::collections::{HashMap, HashSet};

use super::BlockTaskInputs;

/// A block task sink contains all the block tasks belonging to a pipeline task.

pub struct BlockTaskSink {
    pub pipeline_task_id: String,
    /// key: block_task_id
    pub block_tasks: HashMap<String, BlockTask>,

    pub active_block_tasks: HashSet<String>,
}

impl BlockTaskSink {
    pub fn new(pipeline_task_id: &str) -> Self {
        BlockTaskSink {
            pipeline_task_id: pipeline_task_id.to_owned(),
            block_tasks: HashMap::new(),
            active_block_tasks: HashSet::new(),
        }
    }

    /// Get a block task reference by block task id.
    pub fn get_block_task(&self, block_task_id: &str) -> Option<&BlockTask> {
        self.block_tasks.get(block_task_id)
    }

    /// Create a block task from a block.
    pub fn create(&mut self, block_id: &str) -> String {
        let block_task = BlockTask::new(block_id, &self.pipeline_task_id);
        let block_task_id = block_task.task_id.to_owned();

        self.block_tasks
            .insert(block_task_id.to_owned(), block_task);
        self.active_block_tasks.insert(block_task_id.to_owned());

        block_task_id
    }

    /// Mark a block task as success.
    pub fn update_inputs(&mut self, block_task_id: &str, inputs: Option<BlockTaskInputs>) {
        if let Some(block_task) = self.block_tasks.get_mut(block_task_id) {
            block_task.inputs = inputs;
        }
    }

    /// Mark a block task as success.
    pub fn done(&mut self, block_task_id: &str) {
        if let Some(block_task) = self.block_tasks.get_mut(block_task_id) {
            block_task.done();
        }

        self.active_block_tasks.remove(block_task_id);
    }

    /// Removes a block task, returning the block task at the id if it was previously exists.
    pub fn remove(&mut self, block_task_id: &str) -> Option<BlockTask> {
        self.block_tasks.remove(block_task_id)
    }
}
