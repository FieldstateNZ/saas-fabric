//! `POST /query` request types.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::wire::NdcExpression;

/// The body of `POST /query`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcQueryRequest {
    /// The collection to read.
    pub(crate) collection: String,

    /// The query syntax tree.
    pub(crate) query: NdcQuery,

    /// Values for collection-level arguments. Always empty for us: the Data API
    /// does not expose parameterised collections.
    pub(crate) arguments: BTreeMap<String, Value>,

    /// Relationships involved. Always empty — the Data API does not expose
    /// joins, so there is nothing to declare.
    pub(crate) collection_relationships: BTreeMap<String, Value>,

    /// Variable sets, for the `query.variables` capability. Unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) variables: Option<Vec<BTreeMap<String, Value>>>,

    /// **Request-level arguments.**
    ///
    /// This is the field the whole multi-tenant design rests on: it is how one
    /// request tells a shared connector which tenant's connection to use.
    /// Introduced in NDC 0.2.4 for precisely this kind of whole-request value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_arguments: Option<BTreeMap<String, Value>>,
}

/// The query tree itself.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcQuery {
    /// Aggregates to compute. Unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) aggregates: Option<BTreeMap<String, Value>>,

    /// Fields to return, keyed by response alias.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) fields: Option<BTreeMap<String, NdcField>>,

    /// Maximum rows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) limit: Option<u32>,

    /// Rows to skip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) offset: Option<u32>,

    /// Ordering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) order_by: Option<NdcOrderBy>,

    /// The predicate.
    ///
    /// Note the name: NDC calls this `predicate`, not `where` or `filter`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) predicate: Option<NdcExpression>,

    /// Grouping. Unused.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) groups: Option<Value>,
}

impl NdcQuery {
    /// An empty query — no projection, predicate, ordering, or paging.
    pub(crate) const fn empty() -> Self {
        Self {
            aggregates: None,
            fields: None,
            limit: None,
            offset: None,
            order_by: None,
            predicate: None,
            groups: None,
        }
    }
}

/// One requested field.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcField {
    /// A plain column.
    Column {
        /// The column name.
        column: String,
        /// Nested selection. Unused.
        #[serde(skip_serializing_if = "Option::is_none")]
        fields: Option<Value>,
    },
}

/// An ordering specification.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcOrderBy {
    /// Elements in priority order.
    pub(crate) elements: Vec<NdcOrderByElement>,
}

/// One ordering element.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcOrderByElement {
    /// Which way.
    pub(crate) order_direction: NdcOrderDirection,
    /// What to order by.
    pub(crate) target: NdcOrderByTarget,
}

/// Sort direction, spelled as NDC spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NdcOrderDirection {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

/// What an ordering element targets.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcOrderByTarget {
    /// A column on the current collection.
    Column {
        /// Relationship path. Always empty for us.
        path: Vec<Value>,
        /// The column name.
        name: String,
        /// Nested field path. Unused.
        #[serde(skip_serializing_if = "Option::is_none")]
        field_path: Option<Vec<String>>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_request_serialises_to_the_shape_the_specification_describes() {
        let request = NdcQueryRequest {
            collection: "customers".to_owned(),
            query: NdcQuery {
                fields: Some(BTreeMap::from([(
                    "id".to_owned(),
                    NdcField::Column {
                        column: "id".to_owned(),
                        fields: None,
                    },
                )])),
                limit: Some(10),
                ..NdcQuery::empty()
            },
            arguments: BTreeMap::new(),
            collection_relationships: BTreeMap::new(),
            variables: None,
            request_arguments: Some(BTreeMap::from([(
                "connection_name".to_owned(),
                Value::String("acme-prod".to_owned()),
            )])),
        };

        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["collection"], "customers");
        assert_eq!(json["query"]["fields"]["id"]["type"], "column");
        assert_eq!(json["query"]["fields"]["id"]["column"], "id");
        assert_eq!(json["query"]["limit"], 10);
        assert_eq!(json["request_arguments"]["connection_name"], "acme-prod");
    }

    #[test]
    fn unused_query_members_are_omitted_rather_than_sent_as_null() {
        let request = NdcQueryRequest {
            collection: "customers".to_owned(),
            query: NdcQuery::empty(),
            arguments: BTreeMap::new(),
            collection_relationships: BTreeMap::new(),
            variables: None,
            request_arguments: None,
        };

        let json = serde_json::to_value(&request).unwrap();

        assert!(json.get("variables").is_none());
        assert!(json.get("request_arguments").is_none());
        assert!(json["query"].get("predicate").is_none());
    }

    #[test]
    fn order_direction_serialises_as_asc_and_desc() {
        assert_eq!(serde_json::to_value(NdcOrderDirection::Asc).unwrap(), "asc");
        assert_eq!(serde_json::to_value(NdcOrderDirection::Desc).unwrap(), "desc");
    }
}
