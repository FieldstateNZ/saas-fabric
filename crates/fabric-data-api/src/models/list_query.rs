//! Query-string parsing for list requests.

use std::collections::BTreeMap;

use fabric_connector::{ComparisonOperator, FieldName, Filter, SortDirection, SortField};
use serde_json::Value;

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
/// This is deliberately modest. A richer query language would have to be
/// expressible by every connector the platform ever talks to, and the moment it
/// is not, the choice is between refusing common queries and translating them
/// unfaithfully. Equality, ordering, projection, and paging are the operations
/// every backend can do exactly.
///
/// Callers needing more should get a purpose-built resource in the catalogue,
/// which is a better answer anyway: it can be indexed, reviewed, and authorised
/// on its own terms.
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
    /// Every field named anywhere in the query is checked against the
    /// resource's `queryable_fields`. That check covers filters as well as
    /// projections, because filtering is an information channel in its own
    /// right: a caller can learn a hidden column's value by narrowing a filter
    /// until rows stop coming back.
    ///
    /// # Errors
    ///
    /// [`DataApiError::BadRequest`] for an unparseable value, an invalid field
    /// name, or a field the resource does not expose.
    pub fn parse(raw: &str, resource: &ResourceDefinition) -> Result<Self, DataApiError> {
        let mut query = Self::default();

        for (key, value) in parse_pairs(raw) {
            match key.as_str() {
                "limit" => query.limit = Some(parse_number(&value, "limit")?),
                "offset" => query.offset = Some(parse_number(&value, "offset")?),
                "sort" => query.sort = parse_sort(&value, resource)?,
                "select" => query.select = parse_select(&value, resource)?,
                _ => {
                    let field = field_name(&key, resource)?;
                    query.filters.insert(field, value);
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

/// Splits a query string into decoded key/value pairs.
///
/// Percent-decoding is done here rather than pulled in from a crate: the rules
/// are short, and a dependency for `%20` is not worth the supply chain.
fn parse_pairs(raw: &str) -> Vec<(String, String)> {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

/// Decodes `+` and `%XX` escapes, leaving anything malformed as written.
fn percent_decode(input: &str) -> String {
    let bytes = input.replace('+', " ").into_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes.get(index) {
            Some(b'%') => {
                let hex = bytes.get(index + 1..index + 3).and_then(|pair| {
                    std::str::from_utf8(pair)
                        .ok()
                        .and_then(|text| u8::from_str_radix(text, 16).ok())
                });

                if let Some(byte) = hex {
                    decoded.push(byte);
                    index += 3;
                } else {
                    // A stray `%` that is not a valid escape is kept as
                    // written rather than dropped, so a malformed value fails
                    // field validation instead of silently changing meaning.
                    decoded.push(b'%');
                    index += 1;
                }
            }
            Some(byte) => {
                decoded.push(*byte);
                index += 1;
            }
            None => break,
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
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
                field: field_name(name, resource)?,
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
        .map(|entry| field_name(entry, resource))
        .collect()
}

/// Validates a caller-supplied field name and checks it is exposed.
///
/// The two checks are deliberately together: every caller-supplied field name in
/// the request goes through here, so there is one place to look when asking
/// "can a caller reference an arbitrary column?".
fn field_name(raw: &str, resource: &ResourceDefinition) -> Result<FieldName, DataApiError> {
    let field = FieldName::try_new(raw.trim())
        .map_err(|error| DataApiError::BadRequest(format!("invalid field name: {error}")))?;

    if !resource.permits_field(&field) {
        return Err(DataApiError::BadRequest(format!("unknown field {field}")));
    }

    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_resource() -> ResourceDefinition {
        serde_json::from_str(r#"{"data_source":"primary","collection":"customers"}"#).unwrap()
    }

    fn restricted_resource() -> ResourceDefinition {
        serde_json::from_str(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        )
        .unwrap()
    }

    #[test]
    fn an_empty_query_string_parses_to_nothing() {
        let query = ListQuery::parse("", &open_resource()).unwrap();

        assert!(query.filters.is_empty());
        assert!(query.to_filter().is_none());
    }

    #[test]
    fn an_unreserved_parameter_becomes_an_equality_filter() {
        let query = ListQuery::parse("status=active", &open_resource()).unwrap();

        assert_eq!(query.filters[&FieldName::try_new("status").unwrap()], "active");
    }

    #[test]
    fn several_filters_combine_with_and() {
        let query = ListQuery::parse("status=active&region=au", &open_resource()).unwrap();

        let Some(Filter::And { clauses }) = query.to_filter() else {
            panic!("expected a conjunction");
        };
        assert_eq!(clauses.len(), 2);
    }

    #[test]
    fn paging_parameters_are_reserved_and_not_treated_as_filters() {
        let query = ListQuery::parse("limit=25&offset=50", &open_resource()).unwrap();

        assert_eq!(query.limit, Some(25));
        assert_eq!(query.offset, Some(50));
        assert!(query.filters.is_empty());
    }

    #[test]
    fn a_leading_minus_sorts_descending() {
        let query = ListQuery::parse("sort=-created_at", &open_resource()).unwrap();

        assert_eq!(query.sort.first().unwrap().direction, SortDirection::Descending);
        assert_eq!(query.sort.first().unwrap().field.as_str(), "created_at");
    }

    #[test]
    fn sort_accepts_several_fields_in_priority_order() {
        let query = ListQuery::parse("sort=region,-created_at", &open_resource()).unwrap();

        assert_eq!(query.sort.len(), 2);
        assert_eq!(query.sort.first().unwrap().direction, SortDirection::Ascending);
    }

    #[test]
    fn select_limits_the_projection() {
        let query = ListQuery::parse("select=id,name", &open_resource()).unwrap();

        assert_eq!(query.select.len(), 2);
    }

    #[test]
    fn percent_encoded_values_are_decoded() {
        let query = ListQuery::parse("name=Alice%20Smith", &open_resource()).unwrap();

        assert_eq!(query.filters[&FieldName::try_new("name").unwrap()], "Alice Smith");
    }

    #[test]
    fn a_plus_is_decoded_as_a_space() {
        let query = ListQuery::parse("name=Alice+Smith", &open_resource()).unwrap();

        assert_eq!(query.filters[&FieldName::try_new("name").unwrap()], "Alice Smith");
    }

    #[test]
    fn a_field_the_resource_hides_cannot_be_filtered_on() {
        // Filtering is an information channel even without projection: narrow
        // the filter until rows disappear and you have read the value.
        let error = ListQuery::parse("salary=100000", &restricted_resource()).unwrap_err();

        assert!(matches!(error, DataApiError::BadRequest(_)));
    }

    #[test]
    fn a_field_the_resource_hides_cannot_be_sorted_on() {
        assert!(ListQuery::parse("sort=salary", &restricted_resource()).is_err());
    }

    #[test]
    fn a_field_the_resource_hides_cannot_be_selected() {
        assert!(ListQuery::parse("select=salary", &restricted_resource()).is_err());
    }

    #[test]
    fn a_field_name_that_is_not_a_valid_identifier_is_rejected() {
        let error = ListQuery::parse("drop%20table=1", &open_resource()).unwrap_err();

        assert!(matches!(error, DataApiError::BadRequest(_)));
    }

    #[test]
    fn a_non_numeric_limit_is_rejected() {
        assert!(ListQuery::parse("limit=lots", &open_resource()).is_err());
    }
}
