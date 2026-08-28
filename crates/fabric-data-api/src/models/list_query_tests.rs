//! Parsing a list request, and what a caller may not reach.

use fabric_connector::{FieldName, Filter, SortDirection};

use crate::{DataApiError, ListQuery, ResourceDefinition};

fn open_resource() -> ResourceDefinition {
    serde_json::from_str(r#"{"data_source":"primary","collection":"customers"}"#).unwrap()
}

fn restricted_resource() -> ResourceDefinition {
    serde_json::from_str(
        r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
    )
    .unwrap()
}

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
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

    assert_eq!(query.filters[&field("status")], "active");
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
    assert_eq!(
        ListQuery::parse("select=id,name", &open_resource())
            .unwrap()
            .select
            .len(),
        2
    );
}

#[test]
fn a_field_the_resource_hides_cannot_be_filtered_on() {
    // Filtering is an information channel even without projection: narrow the
    // filter until rows disappear and you have read the value.
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
    assert!(ListQuery::parse("drop%20table=1", &open_resource()).is_err());
}

#[test]
fn a_non_numeric_limit_is_rejected() {
    assert!(ListQuery::parse("limit=lots", &open_resource()).is_err());
}
