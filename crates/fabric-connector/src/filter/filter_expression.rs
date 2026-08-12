//! The predicate tree itself.

use serde_json::Value;

use crate::{ComparisonOperator, FieldName};

/// A predicate over a collection's fields.
///
/// Values are `serde_json::Value` rather than a bespoke value type: the Data
/// API receives JSON, connectors speak JSON, and inventing a third
/// representation in between would only add two lossy conversions.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Filter {
    /// Every clause must hold. An empty list is vacuously true.
    And {
        /// The clauses to combine.
        clauses: Vec<Filter>,
    },

    /// At least one clause must hold. An empty list is vacuously false.
    Or {
        /// The alternatives.
        clauses: Vec<Filter>,
    },

    /// The inner predicate must not hold.
    Not {
        /// The predicate to negate.
        clause: Box<Filter>,
    },

    /// Compare a field against a literal.
    Compare {
        /// The field to compare.
        field: FieldName,
        /// How to compare it.
        operator: ComparisonOperator,
        /// The literal to compare against.
        value: Value,
    },

    /// The field is null.
    IsNull {
        /// The field to test.
        field: FieldName,
    },

    /// The field's value is one of a set.
    In {
        /// The field to test.
        field: FieldName,
        /// The permitted values. An empty set matches nothing.
        values: Vec<Value>,
    },
}

impl Filter {
    /// Combines two predicates with `AND`, flattening where possible.
    ///
    /// This is how a tenant isolation predicate is joined to a caller's filter
    /// (see [`IsolationModel::tenant_predicate`](crate::IsolationModel)).
    /// Flattening keeps the tree shallow, which keeps the generated query
    /// readable when someone is debugging why a row did or did not come back.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::And { clauses: mut left }, Self::And { clauses: right }) => {
                left.extend(right);
                Self::And { clauses: left }
            }
            (Self::And { clauses: mut left }, right) => {
                left.push(right);
                Self::And { clauses: left }
            }
            (left, Self::And { clauses: right }) => {
                let mut clauses = vec![left];
                clauses.extend(right);
                Self::And { clauses }
            }
            (left, right) => Self::And {
                clauses: vec![left, right],
            },
        }
    }

    /// Every field this predicate mentions.
    ///
    /// Used to check a filter against the connector's schema before executing:
    /// a filter naming a field that does not exist should be a clean rejection,
    /// not a backend error with an unpredictable message.
    #[must_use]
    pub fn referenced_fields(&self) -> Vec<&FieldName> {
        let mut fields = Vec::new();
        self.collect_fields(&mut fields);
        fields
    }

    /// Walks the tree accumulating field references.
    fn collect_fields<'a>(&'a self, into: &mut Vec<&'a FieldName>) {
        match self {
            Self::And { clauses } | Self::Or { clauses } => {
                for clause in clauses {
                    clause.collect_fields(into);
                }
            }
            Self::Not { clause } => clause.collect_fields(into),
            Self::Compare { field, .. } | Self::IsNull { field } | Self::In { field, .. } => {
                into.push(field);
            }
        }
    }

    /// Every distinct operator this predicate uses.
    ///
    /// Checked against connector capabilities before execution.
    #[must_use]
    pub fn referenced_operators(&self) -> Vec<ComparisonOperator> {
        let mut operators = Vec::new();
        self.collect_operators(&mut operators);
        operators.sort_unstable();
        operators.dedup();
        operators
    }

    /// Walks the tree accumulating operators.
    fn collect_operators(&self, into: &mut Vec<ComparisonOperator>) {
        match self {
            Self::And { clauses } | Self::Or { clauses } => {
                for clause in clauses {
                    clause.collect_operators(into);
                }
            }
            Self::Not { clause } => clause.collect_operators(into),
            Self::Compare { operator, .. } => into.push(*operator),
            Self::IsNull { .. } | Self::In { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compare(field: &str, operator: ComparisonOperator) -> Filter {
        Filter::Compare {
            field: FieldName::try_new(field).unwrap(),
            operator,
            value: Value::Bool(true),
        }
    }

    #[test]
    fn combining_two_leaves_produces_a_two_clause_conjunction() {
        let combined = compare("a", ComparisonOperator::Equal).and(compare("b", ComparisonOperator::Equal));

        let Filter::And { clauses } = combined else {
            panic!("expected a conjunction");
        };
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn combining_flattens_rather_than_nesting() {
        // A tenant predicate joined onto an existing conjunction must not
        // produce And[And[..], ..] — that nests one level deeper on every call.
        let existing = compare("a", ComparisonOperator::Equal).and(compare("b", ComparisonOperator::Equal));
        let combined = existing.and(compare("tenant", ComparisonOperator::Equal));

        let Filter::And { clauses } = combined else {
            panic!("expected a conjunction");
        };
        assert_eq!(clauses.len(), 3);
    }

    #[test]
    fn collects_fields_from_nested_clauses() {
        let filter = Filter::Not {
            clause: Box::new(Filter::Or {
                clauses: vec![
                    compare("a", ComparisonOperator::Equal),
                    Filter::IsNull {
                        field: FieldName::try_new("b").unwrap(),
                    },
                ],
            }),
        };

        let fields: Vec<&str> = filter.referenced_fields().iter().map(|f| f.as_str()).collect();
        assert_eq!(fields, ["a", "b"]);
    }

    #[test]
    fn collects_distinct_operators() {
        let filter = compare("a", ComparisonOperator::Contains)
            .and(compare("b", ComparisonOperator::Contains))
            .and(compare("c", ComparisonOperator::GreaterThan));

        assert_eq!(
            filter.referenced_operators(),
            [ComparisonOperator::GreaterThan, ComparisonOperator::Contains]
        );
    }
}
