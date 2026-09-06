//! The `fields` selection on a `POST /mutation` procedure invocation.
//!
//! Split out of `mutation.rs` (`docs/architecture/file-size-policy.md`): a
//! procedure's argument shapes and its result-field-selection shapes are two
//! different concepts that happen to share a request body, and each is
//! substantial enough on its own -- an object/array union, its one recursive
//! case, and the one selection this crate actually builds -- to be read and
//! reviewed independently of `NdcMutationRequest`/`NdcMutationOperation`.

use std::collections::BTreeMap;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a real `ndc-postgres` v3.1.0 request, checked in under
    /// `tests/fixtures/` -- see the README there for how it was captured.
    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/ndc-postgres-v3.1.0/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
    }

    /// The `fields` member of a checked-in request, as the value this
    /// crate's own serialisation must match exactly.
    fn accepted_fields(request_fixture: &str) -> serde_json::Value {
        let request: serde_json::Value = serde_json::from_str(&fixture(request_fixture)).unwrap();
        request["operations"][0]["fields"].clone()
    }

    /// Pins [`NdcMutationFields::affected_rows_only`] against the exact
    /// accepted request body -- `tests/fixtures/ndc-postgres-v3.1.0/request-insert-affected-only.json`,
    /// extracted from the plan's `probe6.sh` (the request that produced
    /// `mutation-insert-affected-only.json`, and, with a `pre_check`
    /// predicate substituted, the request behind
    /// `mutation-delete-other-tenant.json`). Not guessed at: this is the
    /// literal accepted body, read from the fixture rather than retyped.
    #[test]
    fn affected_rows_only_serialises_to_the_accepted_shape() {
        let json = serde_json::to_value(NdcMutationFields::affected_rows_only()).unwrap();

        assert_eq!(json, accepted_fields("request-insert-affected-only.json"));
    }

    /// Pins the type this crate does not yet build — but must still be able
    /// to represent faithfully — against
    /// `tests/fixtures/ndc-postgres-v3.1.0/request-insert-returning.json`,
    /// the request that produced `mutation-insert-ok.json`. Nothing in
    /// `translate::mutation` builds this today (see
    /// [`NdcMutationFields::affected_rows_only`]'s rustdoc for why); this
    /// test exists so the type itself is checked against the capture,
    /// independent of whether anything constructs it yet.
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

        assert_eq!(json, accepted_fields("request-insert-returning.json"));
    }
}
