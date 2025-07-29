use std::{fmt::Debug, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// {"__OOMOL_TYPE__": "oomol/var" | "oomol/secret" | "oomol/bin"}
pub const OOMOL_VAR_DATA: &str = "oomol/var";
pub const OOMOL_SECRET_DATA: &str = "oomol/secret";
pub const OOMOL_BIN_DATA: &str = "oomol/bin";

pub const OOMOL_TYPE_KEY: &str = "__OOMOL_TYPE__";
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OutputRef {
    pub session_id: String,
    pub job_id: String,
    pub handle: String,
    pub executor: String,
}

enum CustomTypes {
    Plain,
    OomolVar,
    OomolSecret,
    OomolBin,
    Unknown,
}

#[derive(Clone)]
pub struct OutputValue {
    pub value: JsonValue,
    pub is_json_serializable: bool,
}

impl OutputValue {
    pub fn new(value: JsonValue, is_json_serializable: bool) -> Self {
        OutputValue {
            value,
            is_json_serializable,
        }
    }

    // here we ignore is_json_serializable's value (we calculate it base on internal value), because some is_json_serializable:false value(oomol/var) can be deserializable from serialized path.
    pub fn deserializable(&self) -> bool {
        match self.value_type() {
            CustomTypes::Plain => true,
            CustomTypes::OomolVar => self
                .serialize_path()
                .is_some_and(|p| PathBuf::from(p).exists()),
            _ => false,
        }
    }

    pub fn maybe_serializable(&self) -> bool {
        if self.is_json_serializable {
            return true;
        }

        match self.value_type() {
            CustomTypes::OomolVar => self.serialize_path().is_some(),
            CustomTypes::OomolSecret => self.serialize_path().is_some(),
            CustomTypes::OomolBin => self.serialize_path().is_some(),
            _ => false,
        }
    }

    fn value_type(&self) -> CustomTypes {
        let obj = match self.value.as_object() {
            Some(obj) => obj,
            None => return CustomTypes::Plain,
        };

        let oomol_type = match obj.get(OOMOL_TYPE_KEY) {
            Some(oomol_type) if oomol_type.is_string() => oomol_type,
            _ => return CustomTypes::Plain,
        };

        match oomol_type.as_str() {
            Some(OOMOL_VAR_DATA) => CustomTypes::OomolVar,
            Some(OOMOL_SECRET_DATA) => CustomTypes::OomolSecret,
            Some(OOMOL_BIN_DATA) => CustomTypes::OomolBin,
            _ => CustomTypes::Unknown,
        }
    }

    fn serialize_path(&self) -> Option<&str> {
        self.value
            .as_object()
            .and_then(|obj| obj.get("serialize_path"))
            .and_then(JsonValue::as_str)
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
            is_json_serializable: !(value.is_object() && value.get(OOMOL_TYPE_KEY).is_some()),
            value,
        })
    }
}
