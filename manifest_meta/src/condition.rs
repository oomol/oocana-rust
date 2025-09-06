use std::collections::HashMap;

use manifest_reader::{
    manifest::{self, ConditionHandleDef, DefaultConditionHandleDef, HandleName},
    JsonValue,
};

#[derive(Debug, Clone)]
pub struct ConditionBlock {
    pub description: Option<String>,
    pub cases: Vec<ConditionHandleDef>,
    pub default: Option<DefaultConditionHandleDef>,
}

impl ConditionBlock {
    pub fn from_manifest(manifest: manifest::ConditionBlock) -> Self {
        let manifest::ConditionBlock {
            description,
            cases,
            default,
        } = manifest;
        Self {
            description,
            cases: cases.unwrap_or_default(),
            default,
        }
    }
}

impl ConditionBlock {
    pub fn evaluate(&self, value_map: &HashMap<HandleName, JsonValue>) -> Option<HandleName> {
        for case in self.cases.iter() {
            if case.is_match(value_map) {
                return Some(case.handle.clone());
            }
        }
        if let Some(default_case) = &self.default {
            return Some(default_case.handle.clone());
        }
        None
    }
}
