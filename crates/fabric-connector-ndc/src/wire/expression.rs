//! NDC predicate types.

use serde_json::Value;

/// An NDC predicate.
///
/// Only the variants the platform emits are modelled. NDC also has
/// `array_comparison` and `exists`, which the Data API does not expose.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcExpression {
    /// All sub-expressions must hold.
    And {
        /// The conjuncts.
        expressions: Vec<NdcExpression>,
    },

    /// At least one sub-expression must hold.
    Or {
        /// The disjuncts.
        expressions: Vec<NdcExpression>,
    },

    /// The sub-expression must not hold.
    Not {
        /// The negated expression.
        expression: Box<NdcExpression>,
    },

    /// A one-sided comparison, such as a null check.
    UnaryComparisonOperator {
        /// The column tested.
        column: NdcComparisonTarget,
        /// Which unary operator.
        operator: NdcUnaryOperator,
    },

    /// A comparison between a column and a value.
    ///
    /// The operator is a **string**, because connectors name their own
    /// operators. The name is looked up from the connector's schema rather than
    /// hardcoded — see [`SchemaIndex`](crate::SchemaIndex).
    BinaryComparisonOperator {
        /// The column compared.
        column: NdcComparisonTarget,
        /// The connector's own name for the operator.
        operator: String,
        /// What to compare against.
        value: NdcComparisonValue,
    },
}

/// The only unary operator NDC defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NdcUnaryOperator {
    /// The column is null.
    IsNull,
}

/// What a comparison targets.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcComparisonTarget {
    /// A column on the current collection.
    Column {
        /// The column name.
        name: String,
        /// Nested field path. Unused.
        #[serde(skip_serializing_if = "Option::is_none")]
        field_path: Option<Vec<String>>,
    },
}

impl NdcComparisonTarget {
    /// Targets a plain column.
    pub(crate) fn column(name: impl Into<String>) -> Self {
        Self::Column {
            name: name.into(),
            field_path: None,
        }
    }
}

/// The right-hand side of a comparison.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcComparisonValue {
    /// A literal.
    Scalar {
        /// The literal value.
        value: Value,
    },
}

impl NdcComparisonValue {
    /// Compares against a literal.
    pub(crate) const fn scalar(value: Value) -> Self {
        Self::Scalar { value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_binary_comparison_serialises_with_the_connectors_operator_name() {
        let expression = NdcExpression::BinaryComparisonOperator {
            column: NdcComparisonTarget::column("status"),
            operator: "_eq".to_owned(),
            value: NdcComparisonValue::scalar(Value::String("active".to_owned())),
        };

        let json = serde_json::to_value(&expression).unwrap();

        assert_eq!(json["type"], "binary_comparison_operator");
        assert_eq!(json["column"]["type"], "column");
        assert_eq!(json["column"]["name"], "status");
        assert_eq!(json["operator"], "_eq");
        assert_eq!(json["value"]["type"], "scalar");
        assert_eq!(json["value"]["value"], "active");
    }

    #[test]
    fn a_null_check_serialises_as_a_unary_operator() {
        let expression = NdcExpression::UnaryComparisonOperator {
            column: NdcComparisonTarget::column("deleted_at"),
            operator: NdcUnaryOperator::IsNull,
        };

        let json = serde_json::to_value(&expression).unwrap();

        assert_eq!(json["type"], "unary_comparison_operator");
        assert_eq!(json["operator"], "is_null");
    }

    #[test]
    fn a_conjunction_nests_its_expressions() {
        let expression = NdcExpression::And {
            expressions: vec![NdcExpression::UnaryComparisonOperator {
                column: NdcComparisonTarget::column("a"),
                operator: NdcUnaryOperator::IsNull,
            }],
        };

        let json = serde_json::to_value(&expression).unwrap();

        assert_eq!(json["type"], "and");
        assert_eq!(json["expressions"][0]["type"], "unary_comparison_operator");
    }
}
