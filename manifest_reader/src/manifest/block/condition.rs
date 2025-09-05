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
pub enum ExpressionOperator {
    #[serde(rename = "==")]
    Equal,
    #[serde(rename = "!=")]
    NotEqual,
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    #[serde(rename = "<=")]
    LessThanOrEqual,
    #[serde(rename = "is null")]
    Null,
    #[serde(rename = "is not null")]
    NotNull,
    #[serde(rename = "is true")]
    True,
    #[serde(rename = "is false")]
    False,
    #[serde(rename = "contains")]
    /// string contains
    Contains,
    #[serde(rename = "not contains")]
    /// string not contains
    NotContains,
    #[serde(rename = "is empty")]
    IsEmpty,
    #[serde(rename = "is not empty")]
    IsNotEmpty,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "not in")]
    NotIn,
    #[serde(rename = "has key")]
    HasKey,
    #[serde(rename = "not has key")]
    NotHasKey,
    #[serde(rename = "has value")]
    HasValue,
    #[serde(rename = "not has value")]
    NotHasValue,
    #[serde(rename = "starts with")]
    StartsWith,
    #[serde(rename = "ends with")]
    EndsWith,
}

impl ExpressionOperator {
    pub fn compare_values(
        &self,
        left: &serde_json::Value,
        right: &Option<serde_json::Value>,
    ) -> bool {
        match self {
            ExpressionOperator::Equal => right.as_ref().map_or(false, |r| left == r),
            ExpressionOperator::NotEqual => right.as_ref().map_or(false, |r| left != r),
            ExpressionOperator::GreaterThan => {
                if let Some(r) = right {
                    if let (Some(lf), Some(rf)) = (left.as_f64(), r.as_f64()) {
                        return lf > rf;
                    }
                }
                false
            }
            ExpressionOperator::LessThan => {
                if let Some(r) = right {
                    if let (Some(lf), Some(rf)) = (left.as_f64(), r.as_f64()) {
                        return lf < rf;
                    }
                }
                false
            }
            ExpressionOperator::GreaterThanOrEqual => {
                if let Some(r) = right {
                    if let (Some(lf), Some(rf)) = (left.as_f64(), r.as_f64()) {
                        return lf >= rf;
                    }
                }
                false
            }
            ExpressionOperator::LessThanOrEqual => {
                if let Some(r) = right {
                    if let (Some(lf), Some(rf)) = (left.as_f64(), r.as_f64()) {
                        return lf <= rf;
                    }
                }
                false
            }
            ExpressionOperator::Null => left.is_null(),
            ExpressionOperator::NotNull => !left.is_null(),
            ExpressionOperator::True => left.as_bool().unwrap_or(false),
            ExpressionOperator::False => !left.as_bool().unwrap_or(true),
            ExpressionOperator::Contains => {
                if let Some(r) = right {
                    if let Some(ls) = left.as_str() {
                        if let Some(rs) = r.as_str() {
                            return ls.contains(rs);
                        }
                    } else if let Some(la) = left.as_array() {
                        return la.contains(r);
                        // } else if let Some(lo) = left.as_object() {
                        //     return lo.values().any(|v| v == r);
                    }
                }
                false
            }
            ExpressionOperator::NotContains => {
                if let Some(r) = right {
                    if let Some(ls) = left.as_str() {
                        if let Some(rs) = r.as_str() {
                            return !ls.contains(rs);
                        }
                    } else if let Some(la) = left.as_array() {
                        return !la.contains(r);
                        // } else if let Some(lo) = left.as_object() {
                        //     return !lo.values().any(|v| v == r);
                    }
                }
                false
            }
            ExpressionOperator::IsEmpty => {
                if left.is_null() {
                    return true;
                }
                if let Some(s) = left.as_str() {
                    return s.is_empty();
                }
                if let Some(a) = left.as_array() {
                    return a.is_empty();
                }
                if let Some(o) = left.as_object() {
                    return o.is_empty();
                }
                false
            }
            ExpressionOperator::IsNotEmpty => {
                if left.is_null() {
                    return false;
                }
                if let Some(s) = left.as_str() {
                    return !s.is_empty();
                }
                if let Some(a) = left.as_array() {
                    return !a.is_empty();
                }
                if let Some(o) = left.as_object() {
                    return !o.is_empty();
                }
                false
            }
            ExpressionOperator::In => {
                if let Some(r) = right {
                    if let Some(ra) = r.as_array() {
                        return ra.contains(left);
                    }
                }
                false
            }
            ExpressionOperator::NotIn => {
                if let Some(r) = right {
                    if let Some(ra) = r.as_array() {
                        return !ra.contains(left);
                    }
                }
                false
            }
            ExpressionOperator::HasKey => {
                if let Some(r) = right {
                    if let Some(rs) = r.as_str() {
                        if let Some(lo) = left.as_object() {
                            return lo.contains_key(rs);
                        }
                    }
                }
                false
            }
            ExpressionOperator::NotHasKey => {
                if let Some(r) = right {
                    if let Some(rs) = r.as_str() {
                        if let Some(lo) = left.as_object() {
                            return !lo.contains_key(rs);
                        }
                    }
                }
                false
            }
            ExpressionOperator::HasValue => {
                if let Some(r) = right {
                    if let Some(lo) = left.as_object() {
                        return lo.values().any(|v| v == r);
                    }
                }
                false
            }
            ExpressionOperator::NotHasValue => {
                if let Some(r) = right {
                    if let Some(lo) = left.as_object() {
                        return !lo.values().any(|v| v == r);
                    }
                }
                false
            }
            ExpressionOperator::StartsWith => {
                if let Some(r) = right {
                    if let Some(ls) = left.as_str() {
                        if let Some(rs) = r.as_str() {
                            return ls.starts_with(rs);
                        }
                    }
                }
                false
            }
            ExpressionOperator::EndsWith => {
                if let Some(r) = right {
                    if let Some(ls) = left.as_str() {
                        if let Some(rs) = r.as_str() {
                            return ls.ends_with(rs);
                        }
                    }
                }
                false
            }
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum LogicalOperator {
    #[serde(rename = "AND")]
    And,
    #[serde(rename = "OR")]
    Or,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_operator_deserialize() {
        let json_and = r#""AND""#;
        let op_and: LogicalOperator = serde_json::from_str(json_and).unwrap();
        assert_eq!(op_and, LogicalOperator::And);

        let json_or = r#""OR""#;
        let op_or: LogicalOperator = serde_json::from_str(json_or).unwrap();
        assert_eq!(op_or, LogicalOperator::Or);
    }

    #[test]
    fn test_expression_operator_deserialize() {
        let operators = vec![
            (r#""==""#, ExpressionOperator::Equal),
            (r#""!=""#, ExpressionOperator::NotEqual),
            (r#""<""#, ExpressionOperator::LessThan),
            (r#""<=""#, ExpressionOperator::LessThanOrEqual),
            (r#"">""#, ExpressionOperator::GreaterThan),
            (r#"">=""#, ExpressionOperator::GreaterThanOrEqual),
            (r#""is null""#, ExpressionOperator::Null),
            (r#""is not null""#, ExpressionOperator::NotNull),
            (r#""is true""#, ExpressionOperator::True),
            (r#""is false""#, ExpressionOperator::False),
            (r#""contains""#, ExpressionOperator::Contains),
            (r#""not contains""#, ExpressionOperator::NotContains),
            (r#""is empty""#, ExpressionOperator::IsEmpty),
            (r#""is not empty""#, ExpressionOperator::IsNotEmpty),
            (r#""in""#, ExpressionOperator::In),
            (r#""not in""#, ExpressionOperator::NotIn),
            (r#""has key""#, ExpressionOperator::HasKey),
            (r#""not has key""#, ExpressionOperator::NotHasKey),
            (r#""has value""#, ExpressionOperator::HasValue),
            (r#""not has value""#, ExpressionOperator::NotHasValue),
            (r#""starts with""#, ExpressionOperator::StartsWith),
            (r#""ends with""#, ExpressionOperator::EndsWith),
        ];
        for (json_op, expected_op) in operators {
            let result: Result<ExpressionOperator, serde_json::Error> =
                serde_json::from_str(json_op);
            assert!(
                result.is_ok(),
                "Failed to deserialize operator: {} error: {:?}",
                json_op,
                result.err()
            );
            let op: ExpressionOperator = result.unwrap();
            assert_eq!(op, expected_op, "Mismatched operator for: {}", json_op);
        }
    }

    // #[test]
    // fn test_expression_operator() {
    //     // equal
    //     {}
    // }
}
