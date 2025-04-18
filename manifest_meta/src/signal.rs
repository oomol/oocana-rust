use std::collections::HashMap;

use manifest_reader::manifest::NodeId;

pub struct SignalCenter {
    pub after: HashMap<NodeId, Vec<NodeId>>,
    pub notify: HashMap<NodeId, Vec<NodeId>>,
}

impl SignalCenter {
    pub fn new() -> Self {
        Self {
            after: HashMap::new(),
            notify: HashMap::new(),
        }
    }

    pub fn parse_run_after(
        &mut self,
        node_id: &NodeId,
        run_after: &[NodeId],
    ) -> Result<(), String> {
        for id in run_after {
            self.after
                .entry(node_id.clone())
                .or_default()
                .push(id.clone());
            self.notify
                .entry(id.clone())
                .or_default()
                .push(node_id.clone());
        }

        Ok(())
    }
}
