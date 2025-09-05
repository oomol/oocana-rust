use serde::Deserialize;

use crate::manifest::HandleName;

#[derive(Deserialize, Debug, Clone)]
pub struct ConditionBlock {
    pub description: Option<String>,
    pub cases: Option<Vec<ConditionHandleDef>>,
    pub default: Option<ConditionHandleDef>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConditionExpression {
    pub handle: HandleName,
    pub operator: ExpressionOperator,
    pub value: Option<serde_json::Value>,
    pub description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ConditionHandleDef {
    pub handle: HandleName,
    pub description: Option<String>,
    pub logical: Option<LogicalOperator>,
    pub expressions: Vec<ConditionExpression>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum ExpressionOperator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Contains,
    NotContains,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum LogicalOperator {
    And,
    Or,
}
