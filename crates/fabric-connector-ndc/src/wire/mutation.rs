//! `POST /mutation` request and response types.

use std::collections::BTreeMap;

use serde_json::Value;

/// The body of `POST /mutation`.
///
/// # Mutations are procedure calls
///
/// This is the part of NDC that surprises people coming from a CRUD API. Core
/// NDC 0.2 has no generic insert/update/delete: the only mutation operation is
/// **invoking a procedure** that the connector declares in its schema.
/// `ndc-postgres`, for instance, generates procedures like `insert_customers`
/// from its configuration.
///
/// That is a deliberate specification choice — it lets a connector expose
/// exactly the writes its backend can do safely, rather than pretending every
/// datastore supports the same write model. The consequence for us is that
/// mapping a neutral [`MutationSpec`](fabric_connector::MutationSpec) onto a
/// procedure call requires per-collection configuration
/// ([`CollectionProcedures`](crate::CollectionProcedures)), because procedure
/// names and argument shapes are the connector's to choose.
///
/// Where no mapping is configured, the connector reports `mutations: false` and
/// writes are refused. Guessing a procedure name and hoping is not an option
/// when the operation might be a delete.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcMutationRequest {
    /// The operations to perform, in order.
    pub(crate) operations: Vec<NdcMutationOperation>,

    /// Relationships involved. Always empty for us.
    pub(crate) collection_relationships: BTreeMap<String, Value>,

    /// Request-level arguments — the per-tenant connection routing, exactly as
    /// on a query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_arguments: Option<BTreeMap<String, Value>>,
}

/// A single mutation operation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcMutationOperation {
    /// Invoke a procedure.
    Procedure {
        /// The procedure's name in the connector's schema.
        name: String,
        /// Named arguments.
        arguments: BTreeMap<String, Value>,
        /// What to read back from the result.
        ///
        /// **Not optional in practice**: a real `ndc-postgres` refuses a
        /// procedure request that omits this, with a 400 naming
        /// `affected_rows` and `returning` as the only two accepted
        /// selections
        /// (`tests/fixtures/ndc-postgres-v3.1.0/mutation-insert-no-fields-400.json`).
        /// The `Option` stays because the wire format allows omitting it;
        /// this crate always sends `Some(_)` — see
        /// [`NdcMutationFields::affected_rows_only`].
        #[serde(skip_serializing_if = "Option::is_none")]
        fields: Option<NdcMutationFields>,
    },
}

/// A field selection over a procedure's result — NDC's general `NestedField`
/// union, restricted to the shapes observed on the wire: `returning` nests an
/// array of row objects under a column
/// ([`NdcResultField::Column`]'s own `fields`), so both variants appear in the
/// one accepted request
/// (`tests/fixtures/ndc-postgres-v3.1.0/mutation-insert-ok.json`; reproduced
/// in this module's tests).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcMutationFields {
    /// Select named fields off a row-shaped result.
    Object {
        /// Requested fields, keyed by response alias.
        fields: BTreeMap<String, NdcResultField>,
    },
    /// Select every element of an array-shaped result the same way —
    /// `returning`'s own selection, one level down from the procedure's.
    Array {
        /// How to read each element.
        fields: Box<NdcMutationFields>,
    },
}

impl NdcMutationFields {
    /// The selection this adapter always sends today: `affected_rows` alone.
    ///
    /// Asking for `returning` too is accepted (`mutation-insert-ok.json`'s
    /// request), but building it needs a caller who asked for the written
    /// rows back, and [`MutationSpec`](fabric_connector::MutationSpec) has no
    /// such flag on `Insert`, `Update` or `Delete`. With nothing to read that
    /// intent from, this is also the minimal accepted selection, observed in
    /// `mutation-insert-affected-only.json` and
    /// `mutation-delete-other-tenant.json`.
    pub(crate) fn affected_rows_only() -> Self {
        Self::Object {
            fields: BTreeMap::from([(
                "affected_rows".to_owned(),
                NdcResultField::Column {
                    column: "affected_rows".to_owned(),
                    fields: None,
                },
            )]),
        }
    }
}

/// One field requested off an object-shaped result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcResultField {
    /// A plain column, optionally projected further — how `returning` asks
    /// for specific fields of each returned row rather than the whole row.
    Column {
        /// The column name.
        column: String,
        /// Nested selection, for a column whose value is row- or
        /// array-shaped.
        #[serde(skip_serializing_if = "Option::is_none")]
        fields: Option<Box<NdcMutationFields>>,
    },
}

/// The body of a successful `POST /mutation` response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcMutationResponse {
    /// One result per operation, in request order.
    pub(crate) operation_results: Vec<NdcOperationResult>,
}

/// The result of one operation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcOperationResult {
    /// A procedure's return value. Its shape is the procedure's to decide.
    Procedure {
        /// Whatever the procedure returned.
        result: Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mutation_request_serialises_as_a_procedure_invocation() {
        let request = NdcMutationRequest {
            operations: vec![NdcMutationOperation::Procedure {
                name: "insert_customers".to_owned(),
                arguments: BTreeMap::from([("objects".to_owned(), Value::Array(vec![]))]),
                fields: None,
            }],
            collection_relationships: BTreeMap::new(),
            request_arguments: None,
        };

        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["operations"][0]["type"], "procedure");
        assert_eq!(json["operations"][0]["name"], "insert_customers");
        assert!(json["operations"][0]["arguments"]["objects"].is_array());
    }

    #[test]
    fn deserialises_a_procedure_result() {
        let response: NdcMutationResponse = serde_json::from_str(
            r#"{"operation_results":[{"type":"procedure","result":{"affected_rows":3}}]}"#,
        )
        .unwrap();

        let NdcOperationResult::Procedure { result } = response.operation_results.first().unwrap();
        assert_eq!(result["affected_rows"], 3);
    }

    /// Pins [`NdcMutationFields::affected_rows_only`] against the exact
    /// accepted request body — the `fields` member of the second `curl` in
    /// the plan's `probe6.sh`, the request that produced
    /// `tests/fixtures/ndc-postgres-v3.1.0/mutation-insert-affected-only.json`
    /// (and, with a `pre_check` predicate substituted, the request behind
    /// `mutation-delete-other-tenant.json`). Not guessed at: this is the
    /// literal accepted body, reproduced.
    #[test]
    fn affected_rows_only_serialises_to_the_accepted_shape() {
        let json = serde_json::to_value(NdcMutationFields::affected_rows_only()).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "type": "object",
                "fields": {
                    "affected_rows": {"type": "column", "column": "affected_rows"}
                }
            })
        );
    }

    /// Pins the type this crate does not yet build — but must still be able
    /// to represent faithfully — against the `fields` member of the first
    /// `curl` in `probe6.sh`, the request that produced
    /// `mutation-insert-ok.json`. Nothing in `translate::mutation` builds
    /// this today (see [`NdcMutationFields::affected_rows_only`]'s rustdoc
    /// for why); this test exists so the type itself is checked against the
    /// capture, independent of whether anything constructs it yet.
    #[test]
    fn the_returning_shape_this_crate_does_not_yet_build_still_matches_the_capture() {
        let selection = NdcMutationFields::Object {
            fields: BTreeMap::from([
                (
                    "affected_rows".to_owned(),
                    NdcResultField::Column {
                        column: "affected_rows".to_owned(),
                        fields: None,
                    },
                ),
                (
                    "returning".to_owned(),
                    NdcResultField::Column {
                        column: "returning".to_owned(),
                        fields: Some(Box::new(NdcMutationFields::Array {
                            fields: Box::new(NdcMutationFields::Object {
                                fields: BTreeMap::from([
                                    (
                                        "id".to_owned(),
                                        NdcResultField::Column {
                                            column: "id".to_owned(),
                                            fields: None,
                                        },
                                    ),
                                    (
                                        "title".to_owned(),
                                        NdcResultField::Column {
                                            column: "title".to_owned(),
                                            fields: None,
                                        },
                                    ),
                                ]),
                            }),
                        })),
                    },
                ),
            ]),
        };

        let json = serde_json::to_value(selection).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "type": "object",
                "fields": {
                    "affected_rows": {"type": "column", "column": "affected_rows"},
                    "returning": {
                        "type": "column",
                        "column": "returning",
                        "fields": {
                            "type": "array",
                            "fields": {
                                "type": "object",
                                "fields": {
                                    "id": {"type": "column", "column": "id"},
                                    "title": {"type": "column", "column": "title"}
                                }
                            }
                        }
                    }
                }
            })
        );
    }
}
