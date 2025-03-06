use nanoid::nanoid;
use std::time::{SystemTime, UNIX_EPOCH};

mod block_input_sink;
mod block_task_sink;
// mod pipeline_task_sink;

pub use block_input_sink::BlockInputSink;
pub use block_task_sink::BlockTaskSink;
// pub use pipeline_task_sink::PipelineTaskSink;

pub use block_input_sink::BlockTaskInput;
pub use block_input_sink::BlockTaskInputs;

pub enum TaskStatus {
    Running,
    Success,
    // Error,
}

pub struct PipelineTask {
    pub task_id: String,
    pub pipeline_id: String,
    pub status: TaskStatus,
    pub create_at: u128,
    pub finish_at: u128,
}

impl PipelineTask {
    pub fn new(pipeline_id: &str) -> Self {
        Self {
            task_id: nanoid!(),
            pipeline_id: pipeline_id.to_owned(),
            status: TaskStatus::Running,
            create_at: timestamp(),
            finish_at: 0,
        }
    }

    pub fn done(&mut self) {
        self.status = TaskStatus::Success;
        self.finish_at = timestamp();
    }
}

pub struct BlockTask {
    pub task_id: String,
    pub block_id: String,
    pub pipeline_task_id: String,
    pub status: TaskStatus,
    pub create_at: u128,
    pub finish_at: u128,
    pub inputs: Option<BlockTaskInputs>,
}

impl BlockTask {
    pub fn new(block_id: &str, pipeline_task_id: &str) -> Self {
        Self {
            task_id: nanoid!(),
            block_id: block_id.to_owned(),
            pipeline_task_id: pipeline_task_id.to_owned(),
            status: TaskStatus::Running,
            create_at: timestamp(),
            finish_at: 0,
            inputs: None,
        }
    }

    pub fn done(&mut self) {
        self.status = TaskStatus::Success;
        self.finish_at = timestamp();
    }
}

pub fn timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
