use crate::graph::Slot;
use std::collections::HashMap;
use std::rc::Rc;

pub type BlockTaskInput = Rc<serde_json::Value>;

/// key: slot_id, value: input value
pub type BlockTaskInputs = HashMap<String, BlockTaskInput>;

pub struct BlockInputSink {
    pub pipeline_task_id: String,
    /// key: block_id
    pub block_inputs: HashMap<String, Vec<BlockTaskInputs>>,
}

impl BlockInputSink {
    pub fn new(pipeline_task_id: &str) -> Self {
        BlockInputSink {
            pipeline_task_id: pipeline_task_id.to_owned(),
            block_inputs: HashMap::new(),
        }
    }

    /// Pop a block inputs if all required slots are filled
    pub fn pop_block_inputs(
        &mut self, block_id: &str, in_slots: &Vec<Slot>,
    ) -> Option<BlockTaskInputs> {
        let inputs = self.block_inputs.get_mut(block_id)?;
        let first_input = inputs.first()?;
        if in_slots
            .iter()
            .all(|in_slot| first_input.contains_key(&in_slot.id) || in_slot.optional)
        {
            let result = Some(inputs.remove(0));
            if inputs.len() <= 0 {
                self.block_inputs.remove(block_id);
            }
            result
        } else {
            None
        }
    }

    pub fn add_block_input(&mut self, block_id: &str, in_slot_id: &str, value: BlockTaskInput) {
        if !self.block_inputs.contains_key(block_id) {
            self.block_inputs.insert(block_id.to_owned(), Vec::new());
        }
        let inputs = self.block_inputs.get_mut(block_id).unwrap();
        if let Some(index) = inputs
            .iter()
            .position(|args| !args.contains_key(in_slot_id))
        {
            inputs[index].insert(in_slot_id.to_owned(), value);
        } else {
            let mut input = HashMap::new();
            input.insert(in_slot_id.to_owned(), value);
            inputs.push(input);
        }
    }
}
