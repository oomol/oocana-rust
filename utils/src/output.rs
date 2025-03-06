use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputRef {
    pub session_id: String,
    pub job_id: String,
    pub handle: String,
    pub executor: String,
}

pub trait DropSender: Sync + Send {
    // TODO: drop message 如果单独发 executor 名字有点奇怪，所以直接发 data。但是 data 会导致 scheduler 侧需要再次从 data 中解析出 executor 名字。
    fn drop_message(&self, data: Vec<u8>);
}

#[derive(Clone)]
pub struct OutputValue {
    pub value: JsonValue,
    pub sender: Option<Arc<Box<dyn DropSender>>>,
}

impl Debug for OutputValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputValue")
            .field("value", &self.value)
            .finish()
    }
}

impl Serialize for OutputValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OutputValue {
    fn deserialize<D>(deserializer: D) -> Result<OutputValue, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = JsonValue::deserialize(deserializer)?;
        Ok(OutputValue {
            value,
            sender: None,
        })
    }
}

impl Drop for OutputValue {
    fn drop(&mut self) {
        match &self.value {
            JsonValue::Object(obj) => {
                if let Some(sender) = &self.sender {
                    let data = serde_json::to_vec(obj).unwrap();
                    sender.drop_message(data);
                }
            }
            _ => {}
        }
    }
}
