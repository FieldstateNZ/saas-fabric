//! Tests for query.

use super::query::*;
use crate::wire::NdcSchemaResponse;
use crate::wire::{NdcField, NdcOrderDirection};
use crate::SchemaIndex;
use fabric_connector::QuerySpec;
use fabric_connector::{CollectionName, ComparisonOperator, FieldName, Filter, SortField};
use serde_json::Value;
use std::collections::BTreeMap;

fn index() -> SchemaIndex {
    let schema: NdcSchemaResponse = serde_json::from_str(
        r#"{
            "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
            "object_types": {"customers": {"fields": {
                "id": {"type": {"type": "named", "name": "text"}},
                "tenant_key": {"type": {"type": "named", "name": "text"}}
            }}},
            "collections": [{"name": "customers", "type": "customers"}],
            "procedures": []
        }"#,
    )
    .unwrap();

    SchemaIndex::build(&schema)
}

fn spec() -> QuerySpec {
    QuerySpec::new(CollectionName::try_new("customers").unwrap())
}

#[test]
fn an_empty_projection_asks_for_the_default_columns_not_for_none() {
    // `fields: {}` would return rows with no columns in them.
    let request = to_query_request(&spec(), None, &index()).unwrap();

    assert!(request.query.fields.is_none());
}

#[test]
fn a_projection_becomes_one_column_field_per_name() {
    let spec = spec().with_fields(vec![FieldName::try_new("id").unwrap()]);

    let request = to_query_request(&spec, None, &index()).unwrap();
    let fields = request.query.fields.unwrap();

    assert_eq!(
        fields["id"],
        NdcField::Column {
            column: "id".to_owned(),
            fields: None
        }
    );
}

#[test]
fn sorting_maps_onto_order_by_elements() {
    let spec = spec().with_sort(vec![SortField::descending(FieldName::try_new("id").unwrap())]);

    let order_by = to_query_request(&spec, None, &index())
        .unwrap()
        .query
        .order_by
        .unwrap();

    assert_eq!(order_by.elements.len(), 1);
    assert_eq!(
        order_by.elements.first().unwrap().order_direction,
        NdcOrderDirection::Desc
    );
}

#[test]
fn paging_is_carried_through() {
    let spec = spec().with_paging(Some(25), Some(50));

    let query = to_query_request(&spec, None, &index()).unwrap().query;

    assert_eq!(query.limit, Some(25));
    assert_eq!(query.offset, Some(50));
}

#[test]
fn the_tenant_predicate_reaches_the_wire_request() {
    // The end-to-end check that isolation survives translation.
    let spec = spec().with_filter(Filter::Compare {
        field: FieldName::try_new("tenant_key").unwrap(),
        operator: ComparisonOperator::Equal,
        value: Value::String("tenant-482".to_owned()),
    });

    let request = to_query_request(&spec, None, &index()).unwrap();
    let json = serde_json::to_value(&request).unwrap();

    assert_eq!(json["query"]["predicate"]["operator"], "_eq");
    assert_eq!(json["query"]["predicate"]["value"]["value"], "tenant-482");
}

#[test]
fn routing_arguments_are_placed_on_the_request() {
    let arguments = BTreeMap::from([(
        "connection_name".to_owned(),
        Value::String("acme-prod".to_owned()),
    )]);

    let request = to_query_request(&spec(), Some(arguments), &index()).unwrap();

    assert_eq!(
        request.request_arguments.unwrap()["connection_name"],
        Value::String("acme-prod".to_owned())
    );
}
