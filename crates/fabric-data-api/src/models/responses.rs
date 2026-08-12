//! What the Data API sends back.

use fabric_connector::{QueryOutcome, Row};
use serde_json::{Map, Value};

/// One record, as JSON.
///
/// A plain object rather than a typed struct: the Data API is generic over
/// resources, and the shape of a row is the collection's business.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct RowResponse(Map<String, Value>);

impl From<&Row> for RowResponse {
    fn from(row: &Row) -> Self {
        Self(
            row.as_map()
                .iter()
                .map(|(field, value)| (field.to_string(), value.clone()))
                .collect(),
        )
    }
}

impl RowResponse {
    /// Borrows the underlying object.
    #[must_use]
    pub const fn as_map(&self) -> &Map<String, Value> {
        &self.0
    }
}

/// Paging information for a list response.
///
/// Note the absence of a total count. The connector is not asked for one, so
/// reporting a number would mean inventing it. `has_more` is derived by asking
/// for one row beyond the requested page, which costs nothing and answers the
/// question callers actually have.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PagingInfo {
    /// The limit that was applied.
    pub limit: u32,

    /// The offset that was applied.
    pub offset: u32,

    /// How many rows this page contains.
    pub returned: usize,

    /// Whether at least one more row exists beyond this page.
    pub has_more: bool,
}

/// A list of records.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ListResponse {
    /// The records.
    pub data: Vec<RowResponse>,

    /// Where this page sits.
    pub paging: PagingInfo,
}

impl ListResponse {
    /// Builds a list response, trimming the extra probe row.
    ///
    /// `outcome` is expected to hold up to `limit + 1` rows: the query asks for
    /// one more than the caller wanted so that `has_more` is a fact rather than
    /// a guess. The extra row is removed here and never reaches the caller.
    #[must_use]
    pub fn from_outcome(outcome: &QueryOutcome, limit: u32, offset: u32) -> Self {
        let has_more = outcome.rows.len() as u64 > u64::from(limit);

        let data = outcome
            .rows
            .iter()
            .take(limit as usize)
            .map(RowResponse::from)
            .collect::<Vec<_>>();

        Self {
            paging: PagingInfo {
                limit,
                offset,
                returned: data.len(),
                has_more,
            },
            data,
        }
    }
}

/// The result of a write.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WriteResponse {
    /// How many records the operation affected.
    pub affected: u64,

    /// Records the backend returned, if any — typically the written rows with
    /// server-generated keys filled in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<RowResponse>,
}

impl WriteResponse {
    /// Builds a write response from a mutation outcome.
    #[must_use]
    pub fn from_outcome(outcome: &fabric_connector::MutationOutcome) -> Self {
        Self {
            affected: outcome.affected_rows,
            data: outcome.returned_rows.iter().map(RowResponse::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use fabric_connector::FieldName;

    use super::*;

    fn row(id: i64) -> Row {
        Row::new().with(FieldName::try_new("id").unwrap(), Value::from(id))
    }

    #[test]
    fn a_full_page_with_a_probe_row_reports_more_available() {
        // Three rows came back for a limit of two: the third is the probe.
        let outcome = QueryOutcome::from_rows(vec![row(1), row(2), row(3)]);

        let response = ListResponse::from_outcome(&outcome, 2, 0);

        assert_eq!(response.data.len(), 2);
        assert!(response.paging.has_more);
        assert_eq!(response.paging.returned, 2);
    }

    #[test]
    fn the_probe_row_never_reaches_the_caller() {
        let outcome = QueryOutcome::from_rows(vec![row(1), row(2), row(3)]);

        let response = ListResponse::from_outcome(&outcome, 2, 0);

        let ids: Vec<&Value> = response
            .data
            .iter()
            .filter_map(|r| r.as_map().get("id"))
            .collect();
        assert_eq!(ids, [&Value::from(1), &Value::from(2)]);
    }

    #[test]
    fn a_short_page_reports_no_more_available() {
        let outcome = QueryOutcome::from_rows(vec![row(1)]);

        let response = ListResponse::from_outcome(&outcome, 10, 0);

        assert!(!response.paging.has_more);
        assert_eq!(response.paging.returned, 1);
    }

    #[test]
    fn an_exactly_full_page_with_no_probe_reports_no_more() {
        let outcome = QueryOutcome::from_rows(vec![row(1), row(2)]);

        assert!(!ListResponse::from_outcome(&outcome, 2, 0).paging.has_more);
    }

    #[test]
    fn an_empty_result_serialises_as_an_empty_array() {
        let response = ListResponse::from_outcome(&QueryOutcome::default(), 10, 0);

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["data"], Value::Array(vec![]));
        assert_eq!(json["paging"]["returned"], 0);
    }

    #[test]
    fn a_write_response_omits_data_when_nothing_was_returned() {
        let outcome = fabric_connector::MutationOutcome::affected(3);

        let json = serde_json::to_value(WriteResponse::from_outcome(&outcome)).unwrap();

        assert_eq!(json["affected"], 3);
        assert!(json.get("data").is_none());
    }
}
