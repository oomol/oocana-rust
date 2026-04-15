use std::collections::HashMap;

use job::{BlockInputs, BlockJobStacks, JobId};
use manifest_meta::JsonValue;
use serde_json::Value;

use super::{ReporterMessage, ReporterTx};

pub struct BlockReporterTx {
    job_id: JobId,
    block_path: Option<String>,
    stacks: BlockJobStacks,
    tx: ReporterTx,
}

impl BlockReporterTx {
    pub fn new(
        job_id: JobId,
        block_path: Option<String>,
        stacks: BlockJobStacks,
        tx: ReporterTx,
    ) -> Self {
        Self {
            job_id,
            block_path,
            stacks,
            tx,
        }
    }

    pub fn started(&self, inputs: &Option<BlockInputs>) {
        self.tx.send(ReporterMessage::BlockStarted {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            inputs,
            create_at: ReporterMessage::now(),
        });
    }

    pub fn finished(&self, result: Option<HashMap<String, JsonValue>>, error: Option<String>) {
        self.tx.send(ReporterMessage::BlockFinished {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            error,
            result,
            finish_at: ReporterMessage::now(),
        });
    }

    pub fn output(&self, result: &JsonValue, handle: &str) {
        self.tx.send(ReporterMessage::BlockOutput {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            output: result,
            handle,
        });
    }

    pub fn outputs(&self, outputs: &HashMap<String, JsonValue>) {
        self.tx.send(ReporterMessage::BlockOutputs {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            outputs,
        });
    }

    pub fn log(&self, log: &str, stdio: &str) {
        self.tx.send(ReporterMessage::BlockLog {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            log,
            stdio,
        });
    }

    pub fn preview(&self, payload: &serde_json::Value) {
        self.tx.send(ReporterMessage::BlockPreview {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            payload,
            node_id: None,
        });
    }

    pub fn error(&self, error: &str) {
        self.tx.send(ReporterMessage::BlockError {
            session_id: &self.tx.session_id,
            job_id: &self.job_id,
            block_path: &self.block_path,
            stacks: self.stacks.vec(),
            error,
        });
    }

    pub fn forward_remote_log(&self, mut item: Value) {
        if let Some(obj) = item.as_object_mut() {
            // Overwrite top-level coordinates with local values so the
            // event is attributed to the correct block in the local flow.
            // session_id is also required for MQTT subscriber routing.
            obj.insert(
                "session_id".into(),
                Value::String(self.tx.session_id.to_string()),
            );
            obj.insert("job_id".into(), Value::String(self.job_id.to_string()));
            obj.insert(
                "block_path".into(),
                self.block_path
                    .as_ref()
                    .map(|p| Value::String(p.clone()))
                    .unwrap_or(Value::Null),
            );

            // Prepend local stacks to remote stacks
            let remote_stacks = obj
                .get("stacks")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();
            let local_stacks = match serde_json::to_value(self.stacks.vec()) {
                Ok(Value::Array(arr)) => arr,
                Ok(_) => {
                    tracing::warn!("forward_remote_log: local stacks serialized to non-array");
                    vec![]
                }
                Err(e) => {
                    tracing::warn!("forward_remote_log: failed to serialize local stacks: {e}");
                    vec![]
                }
            };
            let mut merged = local_stacks;
            merged.extend(remote_stacks);
            obj.insert("stacks".into(), Value::Array(merged));
        }

        match serde_json::to_vec(&item) {
            Ok(bytes) => self.tx.send_raw(bytes),
            Err(e) => {
                tracing::warn!("forward_remote_log: failed to serialize event: {e}");
            }
        }
    }
}
