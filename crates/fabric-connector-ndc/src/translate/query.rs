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
