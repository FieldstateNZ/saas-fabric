//! `POST /query` response types, and the shared error body.

use std::collections::BTreeMap;

use serde_json::Value;

/// The body of a successful `POST /query` response.
///
/// A **list** of row sets, one per variable set in the request. We never send
/// variables, so a conforming connector returns exactly one — but the type is a
/// list because the protocol says so, and assuming otherwise would break
/// against a connector that padded the list.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct NdcQueryResponse(pub(crate) Vec<NdcRowSet>);

impl NdcQueryResponse {
    /// The first row set, if the connector returned one.
    pub(crate) fn first(&self) -> Option<&NdcRowSet> {
        self.0.first()
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
    #[serde(default)]
    pub(crate) details: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialises_a_row_set() {
        let response: NdcQueryResponse =
            serde_json::from_str(r#"[{"rows":[{"id":1,"name":"Alice"}]}]"#).unwrap();

        let rows = response.first().unwrap().rows.as_ref().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.first().unwrap()["name"], "Alice");
    }

    #[test]
    fn an_absent_rows_member_is_distinct_from_an_empty_one() {
        let no_rows: NdcQueryResponse = serde_json::from_str("[{}]").unwrap();
        let empty_rows: NdcQueryResponse = serde_json::from_str(r#"[{"rows":[]}]"#).unwrap();

        assert!(no_rows.first().unwrap().rows.is_none());
        assert_eq!(empty_rows.first().unwrap().rows.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn deserialises_an_error_body_without_details() {
        let error: NdcErrorResponse =
            serde_json::from_str(r#"{"message":"relation does not exist"}"#).unwrap();

        assert_eq!(error.message, "relation does not exist");
    }
}
