//! Procedures and their declared arguments, indexed by name.

use std::collections::BTreeMap;

use crate::wire::{NdcSchemaResponse, NdcType};

/// Procedure name to the arguments it declares.
pub(super) type ProcedureIndex = BTreeMap<String, BTreeMap<String, ArgumentKind>>;

/// What a declared argument's type permits it to carry.
///
/// A two-way split rather than a copy of NDC's whole type language, because
/// only one distinction is enforceable. A `predicate` argument is the only
/// thing a filter may be sent as — `schema_response.jsonschema` types it
/// `{"type": "predicate", "object_type_name": …}` — so pointing a
/// `filter_argument` at anything else is provably wrong. What a *payload*
/// argument should be is connector-defined (an array of objects here, a named
/// input type there), so no equivalent claim can be made about it, and this
/// type deliberately does not pretend otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgumentKind {
    /// NDC's `predicate` type. A filter may be sent here.
    Predicate,
    /// Anything else. A filter may not.
    Value,
}

impl ArgumentKind {
    /// How this argument reads in an operator-facing message.
    pub(crate) const fn describe(self) -> &'static str {
        match self {
            Self::Predicate => "a predicate",
            Self::Value => "not a predicate",
        }
    }
}

/// Indexes every procedure in a schema response.
pub(super) fn build(schema: &NdcSchemaResponse) -> ProcedureIndex {
    schema
        .procedures
        .iter()
        .map(|procedure| {
            let arguments = procedure
                .arguments
                .iter()
                .map(|(name, info)| (name.clone(), kind_of(&info.argument_type)))
                .collect();

            (procedure.name.clone(), arguments)
        })
        .collect()
}

/// Classifies one declared type.
///
/// A nullable predicate is still a predicate: nullability says the argument may
/// be omitted, not that what it carries changes. Everything else — including an
/// array of predicates, which is not an argument position this crate can fill —
/// is a value.
fn kind_of(argument_type: &NdcType) -> ArgumentKind {
    match argument_type {
        NdcType::Predicate { .. } => ArgumentKind::Predicate,
        NdcType::Nullable { underlying_type } => kind_of(underlying_type),
        NdcType::Named { .. } | NdcType::Array { .. } => ArgumentKind::Value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed(procedures: &str) -> ProcedureIndex {
        let schema: NdcSchemaResponse =
            serde_json::from_str(&format!(r#"{{"procedures": {procedures}}}"#)).unwrap();

        build(&schema)
    }

    #[test]
    fn a_predicate_argument_is_recognised_as_one() {
        let index = indexed(
            r#"[{"name": "delete_customers", "arguments": {
                "filter": {"type": {"type": "predicate", "object_type_name": "customers"}}
            }}]"#,
        );

        assert_eq!(index["delete_customers"]["filter"], ArgumentKind::Predicate);
    }

    #[test]
    fn a_nullable_predicate_is_still_a_predicate() {
        let index = indexed(
            r#"[{"name": "delete_customers", "arguments": {
                "filter": {"type": {"type": "nullable", "underlying_type":
                    {"type": "predicate", "object_type_name": "customers"}}}
            }}]"#,
        );

        assert_eq!(index["delete_customers"]["filter"], ArgumentKind::Predicate);
    }

    #[test]
    fn an_array_argument_is_a_value_not_a_predicate() {
        let index = indexed(
            r#"[{"name": "insert_customers", "arguments": {
                "objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "customers"}}}
            }}]"#,
        );

        assert_eq!(index["insert_customers"]["objects"], ArgumentKind::Value);
    }
}
