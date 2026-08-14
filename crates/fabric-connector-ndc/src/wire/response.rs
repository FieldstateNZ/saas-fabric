//! `POST /query` response types, and the shared error body.

use std::collections::BTreeMap;

use serde_json::Value;

/// The body of a successful `POST /query` response.
///
/// A **list** of row sets, one per variable set in the request. The type is a
/// list because the protocol says so, but the count is not open:
/// `query_response.jsonschema` states it plainly — multiple row sets come back
/// only for a query using variables, and otherwise "there should always be
/// exactly one". This client never sends variables, so anything other than one
/// is a malformed response rather than something to accommodate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct NdcQueryResponse(pub(crate) Vec<NdcRowSet>);

impl NdcQueryResponse {
    /// The single row set a variable-free request must produce.
    ///
    /// `None` for any other count, including *more* than one. Taking the first
    /// of several would discard rows the connector returned with nothing in
    /// the response to say so — the silent-truncation failure this crate's
    /// never-widen rule exists to prevent, and the direction that returns a
    /// plausible-looking `200`.
    pub(crate) fn sole(&self) -> Option<&NdcRowSet> {
        match self.0.as_slice() {
            [row_set] => Some(row_set),
            _ => None,
        }
    }

    /// How many row sets came back, so a refusal can say what was wrong.
    pub(crate) fn count(&self) -> usize {
        self.0.len()
    }
}

/// One set of rows.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcRowSet {
    /// Aggregate results. Unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) aggregates: Option<BTreeMap<String, Value>>,

    /// The rows.
    ///
    /// `None` is meaningful and distinct from `Some(vec![])`: it means the
    /// query requested no fields, not that no rows matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rows: Option<Vec<BTreeMap<String, Value>>>,

    /// Grouping results. Unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) groups: Option<Value>,
}

/// The error body a connector returns on a 4xx or 5xx.
///
/// Both fields are for **internal telemetry only**. They can name physical
/// tables, schemas, and servers, all of which §2 and §29 keep away from
/// applications.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcErrorResponse {
    /// A human-readable summary.
    pub(crate) message: String,

    /// Structured detail.
    ///
    /// Required, because `error_response.jsonschema` lists it alongside
    /// `message` in `required`. A body that omits it is not an NDC error body,
    /// and the honest response to one is to say the connector returned a
    /// status rather than to quote a message from a document we have just
    /// established is not the shape it claims to be — which is exactly what
    /// `client::error_mapping` does when this fails to parse.
    pub(crate) details: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialises_a_row_set() {
        let response: NdcQueryResponse =
            serde_json::from_str(r#"[{"rows":[{"id":1,"name":"Alice"}]}]"#).unwrap();

        let rows = response.sole().unwrap().rows.as_ref().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.first().unwrap()["name"], "Alice");
    }

    #[test]
    fn an_absent_rows_member_is_distinct_from_an_empty_one() {
        let no_rows: NdcQueryResponse = serde_json::from_str("[{}]").unwrap();
        let empty_rows: NdcQueryResponse = serde_json::from_str(r#"[{"rows":[]}]"#).unwrap();

        assert!(no_rows.sole().unwrap().rows.is_none());
        assert_eq!(empty_rows.sole().unwrap().rows.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn more_than_one_row_set_has_no_sole_member() {
        // We send no variables, so a second row set is the connector not
        // answering the question we asked. `.first()` would have handed back
        // one of them and thrown the rest away.
        let response: NdcQueryResponse =
            serde_json::from_str(r#"[{"rows":[{"id":1}]},{"rows":[{"id":2}]}]"#).unwrap();

        assert!(response.sole().is_none());
        assert_eq!(response.count(), 2);
    }

    #[test]
    fn an_error_body_without_details_is_not_an_ndc_error_body() {
        // `details` is required by the specification's own schema.
        let error = serde_json::from_str::<NdcErrorResponse>(r#"{"message":"relation does not exist"}"#);

        assert!(error.is_err());
    }

    #[test]
    fn deserialises_a_conforming_error_body() {
        let error: NdcErrorResponse =
            serde_json::from_str(r#"{"message":"relation does not exist","details":{}}"#).unwrap();

        assert_eq!(error.message, "relation does not exist");
    }

    #[test]
    fn a_null_details_member_still_satisfies_the_schema() {
        // `details` has no type constraint, so `null` is a legitimate value
        // for it -- distinct from omitting the key.
        let error: NdcErrorResponse = serde_json::from_str(r#"{"message":"boom","details":null}"#).unwrap();

        assert_eq!(error.details, Value::Null);
    }
}
