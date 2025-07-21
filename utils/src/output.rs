use std::{fmt::Debug, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// {"__OOMOL_TYPE__": "oomol/var" | "oomol/secret" | "oomol/bin"}
pub const OOMOL_VAR_DATA: &str = "oomol/var";
pub const OOMOL_SECRET_DATA: &str = "oomol/secret";
pub const OOMOL_BIN_DATA: &str = "oomol/bin";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputRef {
    pub session_id: String,
    pub job_id: String,
    pub handle: String,
    pub executor: String,
}

#[derive(Clone)]
pub struct OutputValue {
    pub value: JsonValue,
    // this field won't be serialized
    pub cacheable: bool,
}

impl OutputValue {
    pub fn new(value: JsonValue, cacheable: bool) -> Self {
        OutputValue { value, cacheable }
    }

    pub fn deserializable(&self) -> bool {
        if self.cacheable {
            return true;
        }

        if self.is_oomol_type() {
            if self.is_oomol_type_var()
                && self
                    .serialize_path()
                    .is_some_and(|p| PathBuf::from(p).exists())
            {
                return true;
            } else {
                return false;
            }
        }

        true
    }

    fn is_oomol_type(&self) -> bool {
        self.value.is_object() && self.value.get("oomol_type").is_some()
    }

    fn is_oomol_type_var(&self) -> bool {
        self.value.is_object()
            && self
                .value
                .get("oomol_type")
                .is_some_and(|v| v.is_string() && v.as_str() == Some(OOMOL_VAR_DATA))
    }

    pub fn serialize_path(&self) -> Option<String> {
        self.value
            .as_object()
            .and_then(|obj| obj.get("serialize_path"))
            .and_then(JsonValue::as_str)
            .map(String::from)
    }
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
            cacheable: true,
        })
    }
}
