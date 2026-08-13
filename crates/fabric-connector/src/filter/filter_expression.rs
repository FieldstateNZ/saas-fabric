//! The predicate tree itself.
//!
//! The questions asked *of* a tree — which fields it names, which capabilities
//! a backend needs to run it — live in
//! [`filter_introspection`](super::filter_introspection).

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
}
