//! Procedures and their declared arguments, indexed by name.

use std::collections::BTreeMap;

use crate::wire::{NdcSchemaResponse, NdcType};

/// Procedure name to the arguments it declares.
pub(super) type ProcedureIndex = BTreeMap<String, BTreeMap<String, ArgumentInfo>>;

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
///
/// Nullability is tracked separately, on [`ArgumentInfo`], rather than
/// widening this into a three-way split. Whether an argument is nullable is a
/// question about *presence* — must the caller supply it at all — orthogonal
/// to what it may carry when supplied, and folding the two together would
/// make "is this a predicate" stop being a single pattern match.
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

/// What a procedure declares about one of its arguments: what it may carry,
/// and whether the procedure can be called without it.
///
/// The `required` half exists for one check: a procedure's non-nullable
/// argument that no part of a mapping supplies is refused at startup, rather
/// than left to fail on the connector's first call. `ndc-postgres`'s
/// `key_id` and `key_tenant_key` are exactly this shape — plain, non-nullable
/// `text` arguments a mapping has to know to fill via `key_arguments`, with
/// nothing in the type system forcing it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArgumentInfo {
    /// What the argument may carry.
    pub(crate) kind: ArgumentKind,
    /// Whether the procedure can be called without this argument.
    ///
    /// `false` for anything wrapped in NDC's `nullable` type; `true`
    /// otherwise. A nullable predicate is still absent-able, not
    /// non-predicate — this field says nothing about [`Self::kind`].
    pub(crate) required: bool,
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
                .map(|(name, info)| {
                    (
                        name.clone(),
                        ArgumentInfo {
                            kind: kind_of(&info.argument_type),
                            required: !is_nullable(&info.argument_type),
                        },
                    )
                })
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

/// Whether a declared type is wrapped in NDC's `nullable`.
///
/// Only the outermost wrapper matters: `nullable<nullable<T>>` is not a shape
/// any observed schema uses, and this crate has no reason to define what a
/// second layer of absence would mean.
const fn is_nullable(argument_type: &NdcType) -> bool {
    matches!(argument_type, NdcType::Nullable { .. })
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

        assert_eq!(index["delete_customers"]["filter"].kind, ArgumentKind::Predicate);
    }

    #[test]
    fn a_nullable_predicate_is_still_a_predicate() {
        let index = indexed(
            r#"[{"name": "delete_customers", "arguments": {
                "filter": {"type": {"type": "nullable", "underlying_type":
                    {"type": "predicate", "object_type_name": "customers"}}}
            }}]"#,
        );

        assert_eq!(index["delete_customers"]["filter"].kind, ArgumentKind::Predicate);
    }

    #[test]
    fn an_array_argument_is_a_value_not_a_predicate() {
        let index = indexed(
            r#"[{"name": "insert_customers", "arguments": {
                "objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "customers"}}}
            }}]"#,
        );

        assert_eq!(index["insert_customers"]["objects"].kind, ArgumentKind::Value);
    }

    #[test]
    fn a_plain_named_argument_is_required() {
        // `ndc-postgres`'s `key_id`: `{"type": "named", "name": "text"}`, no
        // `nullable` wrapper — the shape a mapping must cover or the
        // connector refuses every call.
        let index = indexed(
            r#"[{"name": "delete_articles", "arguments": {
                "key_id": {"type": {"type": "named", "name": "text"}}
            }}]"#,
        );

        assert!(index["delete_articles"]["key_id"].required);
    }

    #[test]
    fn a_nullable_argument_is_not_required() {
        let index = indexed(
            r#"[{"name": "delete_articles", "arguments": {
                "pre_check": {"type": {"type": "nullable", "underlying_type":
                    {"type": "predicate", "object_type_name": "articles"}}}
            }}]"#,
        );

        assert!(!index["delete_articles"]["pre_check"].required);
    }

    #[test]
    fn a_bare_array_argument_is_required() {
        // `insert_articles`'s `objects` — bare, not wrapped in `nullable`:
        // nothing in this crate's observed schemas sends an insert with no
        // rows, and `required_arguments_tests` pins that this crate holds a
        // mapping to supplying it.
        let index = indexed(
            r#"[{"name": "insert_articles", "arguments": {
                "objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "articles"}}}
            }}]"#,
        );

        assert!(index["insert_articles"]["objects"].required);
    }

    #[test]
    fn a_bare_predicate_argument_is_required() {
        // A hand-written fixture shape, `delete_customers(filter)` — no
        // capture pins it, and `filter` is the argument name issue #62 found
        // wrong on a real connector. It still proves the point: a `filter`
        // argument with no `nullable` wrapper is required, and a mapping must
        // supply it via `filter_argument` or the connector refuses every
        // call.
        let index = indexed(
            r#"[{"name": "delete_customers", "arguments": {
                "filter": {"type": {"type": "predicate", "object_type_name": "customers"}}
            }}]"#,
        );

        assert!(index["delete_customers"]["filter"].required);
    }
}
