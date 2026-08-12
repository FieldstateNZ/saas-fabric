//! Neutral [`QuerySpec`] to an NDC query request.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, QuerySpec, SortDirection};
use serde_json::Value;

use crate::translate::to_expression;
use crate::wire::{
    NdcField, NdcOrderBy, NdcOrderByElement, NdcOrderByTarget, NdcOrderDirection, NdcQuery, NdcQueryRequest,
};
use crate::SchemaIndex;

/// Builds the `POST /query` body for a targeted query.
///
/// `spec` must already have been through
/// [`QuerySpec::for_target`](fabric_connector::QuerySpec::for_target) — the
/// tenant predicate is applied there, and this function has no way to tell
/// whether it was.
///
/// # Errors
///
/// [`ConnectorError::Unsupported`] if any part of the predicate cannot be
/// expressed by this connector.
pub(crate) fn to_query_request(
    spec: &QuerySpec,
    request_arguments: Option<BTreeMap<String, Value>>,
    index: &SchemaIndex,
) -> Result<NdcQueryRequest, ConnectorError> {
    let predicate = spec
        .filter
        .as_ref()
        .map(|filter| to_expression(&spec.collection, filter, index))
        .transpose()?;

    Ok(NdcQueryRequest {
        collection: spec.collection.to_string(),
        query: NdcQuery {
            fields: projection(spec),
            limit: spec.limit,
            offset: spec.offset,
            order_by: ordering(spec),
            predicate,
            ..NdcQuery::empty()
        },
        arguments: BTreeMap::new(),
        collection_relationships: BTreeMap::new(),
        variables: None,
        request_arguments,
    })
}

/// Builds the field selection.
///
/// An empty neutral projection means "the default projection", which NDC
/// expresses as `fields: null` rather than an empty map — an empty map would
/// ask for no columns at all and return rows with nothing in them.
fn projection(spec: &QuerySpec) -> Option<BTreeMap<String, NdcField>> {
    if spec.fields.is_empty() {
        return None;
    }

    Some(
        spec.fields
            .iter()
            .map(|field| {
                (
                    field.to_string(),
                    NdcField::Column {
                        column: field.to_string(),
                        fields: None,
                    },
                )
            })
            .collect(),
    )
}

/// Builds the ordering, or `None` when the caller asked for none.
fn ordering(spec: &QuerySpec) -> Option<NdcOrderBy> {
    if spec.sort.is_empty() {
        return None;
    }

    Some(NdcOrderBy {
        elements: spec
            .sort
            .iter()
            .map(|sort| NdcOrderByElement {
                order_direction: match sort.direction {
                    SortDirection::Ascending => NdcOrderDirection::Asc,
                    SortDirection::Descending => NdcOrderDirection::Desc,
                },
                target: NdcOrderByTarget::Column {
                    path: Vec::new(),
                    name: sort.field.to_string(),
                    field_path: None,
                },
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use fabric_connector::{CollectionName, ComparisonOperator, FieldName, Filter, SortField};

    use super::*;
    use crate::wire::NdcSchemaResponse;

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
}
