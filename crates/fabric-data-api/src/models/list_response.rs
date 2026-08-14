//! A page of records.

use fabric_connector::QueryOutcome;

use crate::models::VisibleFields;
use crate::RowResponse;

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
    ///
    /// `visible` is not optional and cannot be: every row goes through
    /// [`RowResponse::project`], which needs it to know what this request may
    /// disclose. See that method for why the projection lives in the
    /// constructor rather than at each call site.
    pub(crate) fn from_outcome(
        outcome: &QueryOutcome,
        visible: &VisibleFields<'_>,
        limit: u32,
        offset: u32,
    ) -> Self {
        let has_more = outcome.rows.len() as u64 > u64::from(limit);

        let data = outcome
            .rows
            .iter()
            .take(limit as usize)
            .map(|row| RowResponse::project(row, visible))
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

#[cfg(test)]
mod tests {
    use fabric_connector::{FieldName, IsolationModel, Row};
    use serde_json::Value;

    use super::*;
    use crate::ResourceDefinition;

    fn open() -> ResourceDefinition {
        serde_json::from_str(r#"{"data_source":"primary","collection":"customers"}"#).unwrap()
    }

    fn restricted() -> ResourceDefinition {
        serde_json::from_str(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        )
        .unwrap()
    }

    /// A dedicated placement, so these tests exercise the catalogue rule alone.
    /// The isolation rule has its own tests in `visible_fields`.
    fn dedicated(resource: &ResourceDefinition) -> VisibleFields<'_> {
        VisibleFields::new(resource, &IsolationModel::Database)
    }

    fn row(id: i64) -> Row {
        Row::new().with(FieldName::try_new("id").unwrap(), Value::from(id))
    }

    #[test]
    fn a_full_page_with_a_probe_row_reports_more_available() {
        // Three rows came back for a limit of two: the third is the probe.
        let resource = open();
        let outcome = QueryOutcome::from_rows(vec![row(1), row(2), row(3)]);

        let response = ListResponse::from_outcome(&outcome, &dedicated(&resource), 2, 0);

        assert_eq!(response.data.len(), 2);
        assert!(response.paging.has_more);
        assert_eq!(response.paging.returned, 2);
    }

    #[test]
    fn the_probe_row_never_reaches_the_caller() {
        let resource = open();
        let outcome = QueryOutcome::from_rows(vec![row(1), row(2), row(3)]);

        let response = ListResponse::from_outcome(&outcome, &dedicated(&resource), 2, 0);

        let ids: Vec<&Value> = response
            .data
            .iter()
            .filter_map(|r| r.as_map().get("id"))
            .collect();
        assert_eq!(ids, [&Value::from(1), &Value::from(2)]);
    }

    #[test]
    fn a_short_page_reports_no_more_available() {
        let resource = open();
        let outcome = QueryOutcome::from_rows(vec![row(1)]);

        let response = ListResponse::from_outcome(&outcome, &dedicated(&resource), 10, 0);

        assert!(!response.paging.has_more);
        assert_eq!(response.paging.returned, 1);
    }

    #[test]
    fn an_exactly_full_page_with_no_probe_reports_no_more() {
        let resource = open();
        let outcome = QueryOutcome::from_rows(vec![row(1), row(2)]);

        let response = ListResponse::from_outcome(&outcome, &dedicated(&resource), 2, 0);

        assert!(!response.paging.has_more);
    }

    #[test]
    fn an_empty_result_serialises_as_an_empty_array() {
        let resource = open();
        let outcome = QueryOutcome::default();

        let response = ListResponse::from_outcome(&outcome, &dedicated(&resource), 10, 0);

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["data"], Value::Array(vec![]));
        assert_eq!(json["paging"]["returned"], 0);
    }

    #[test]
    fn every_row_on_the_page_is_projected_not_just_the_first() {
        let resource = restricted();
        let wide = |id: i64| row(id).with(FieldName::try_new("salary").unwrap(), Value::from(190_000));
        let outcome = QueryOutcome::from_rows(vec![wide(1), wide(2)]);

        let response = ListResponse::from_outcome(&outcome, &dedicated(&resource), 10, 0);

        assert!(response
            .data
            .iter()
            .all(|row| !row.as_map().contains_key("salary")));
    }
}
