//! The parsed form of a list request.

use std::collections::BTreeMap;

use fabric_connector::{ComparisonOperator, FieldName, Filter, SortDirection, SortField};
use serde_json::Value;

use crate::models::{field_reference, query_string};
use crate::{DataApiError, ResourceDefinition};

/// A parsed list request.
///
/// # The query language, such as it is
///
/// ```text
/// GET /data/customers?status=active&limit=25&offset=50&sort=-created_at&select=id,name
/// ```
///
/// Four reserved parameters — `limit`, `offset`, `sort`, `select`. **Every
/// other parameter is an equality filter** on the field of that name.
///
/// Deliberately modest. A richer query language would have to be expressible by
/// every connector the platform ever talks to, and the moment it is not, the
/// choice is between refusing common queries and translating them unfaithfully.
/// Equality, ordering, projection, and paging are what every backend does
/// exactly.
///
/// Callers needing more should get a purpose-built catalogue resource — a
/// better answer anyway, since it can be indexed, reviewed, and authorised on
/// its own terms.
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    /// Equality filters, field to value.
    pub filters: BTreeMap<FieldName, String>,

    /// Ordering. A leading `-` means descending.
    pub sort: Vec<SortField>,

    /// Fields to return. Empty means the connector's default projection.
    pub select: Vec<FieldName>,

    /// Maximum rows to return.
    pub limit: Option<u32>,

    /// Rows to skip.
    pub offset: Option<u32>,
}

impl ListQuery {
    /// Parses a raw query string against a resource definition.
    ///
    /// # Errors
    ///
    /// [`DataApiError::BadRequest`] for an unparseable value, an invalid field
    /// name, or a field the resource does not expose.
    pub fn parse(raw: &str, resource: &ResourceDefinition) -> Result<Self, DataApiError> {
        let mut query = Self::default();

        for (key, value) in query_string::parse_pairs(raw) {
            match key.as_str() {
                "limit" => query.limit = Some(parse_number(&value, "limit")?),
                "offset" => query.offset = Some(parse_number(&value, "offset")?),
                "sort" => query.sort = parse_sort(&value, resource)?,
                "select" => query.select = parse_select(&value, resource)?,
                _ => {
                    query
                        .filters
                        .insert(field_reference::parse(&key, resource)?, value);
                }
            }
        }

        Ok(query)
    }

    /// Builds the neutral predicate for these filters.
    ///
    /// Values are treated as strings. The Data API does not know a collection's
    /// column types — that is the connector's knowledge — and guessing that
    /// `"1"` means the number one rather than the string would be wrong as
    /// often as it was right. A connector that needs a typed value coerces it,
    /// which is the layer that actually knows.
    #[must_use]
    pub fn to_filter(&self) -> Option<Filter> {
        let clauses: Vec<Filter> = self
            .filters
            .iter()
            .map(|(field, value)| Filter::Compare {
                field: field.clone(),
                operator: ComparisonOperator::Equal,
                value: Value::String(value.clone()),
            })
            .collect();

        match clauses.len() {
            0 => None,
            1 => clauses.into_iter().next(),
            _ => Some(Filter::And { clauses }),
        }
    }
}

/// Parses a non-negative integer parameter.
fn parse_number(value: &str, parameter: &str) -> Result<u32, DataApiError> {
    value
        .parse()
        .map_err(|_| DataApiError::BadRequest(format!("{parameter} must be a non-negative integer")))
}

/// Parses `sort=-created_at,name`.
fn parse_sort(value: &str, resource: &ResourceDefinition) -> Result<Vec<SortField>, DataApiError> {
    value
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (direction, name) = match entry.strip_prefix('-') {
                Some(rest) => (SortDirection::Descending, rest),
                None => (SortDirection::Ascending, entry.trim_start_matches('+')),
            };

            Ok(SortField {
                field: field_reference::parse(name, resource)?,
                direction,
            })
        })
        .collect()
}

/// Parses `select=id,name`.
fn parse_select(value: &str, resource: &ResourceDefinition) -> Result<Vec<FieldName>, DataApiError> {
    value
        .split(',')
        .filter(|entry| !entry.is_empty())
        .map(|entry| field_reference::parse(entry, resource))
        .collect()
}
