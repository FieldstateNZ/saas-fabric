//! A read operation, in neutral terms.

use crate::{CollectionName, ExecutionTarget, FieldName, Filter, SortField};

/// A read operation against one collection.
///
/// Constructed by the Data API from a caller's request, then passed through
/// [`QuerySpec::for_target`] before it reaches a connector.
#[derive(Debug, Clone, PartialEq)]
pub struct QuerySpec {
    /// The physical collection to read.
    pub collection: CollectionName,

    /// Fields to return. Empty means "whatever the connector considers the
    /// default projection" — usually all fields.
    pub fields: Vec<FieldName>,

    /// The predicate to apply, if any.
    pub filter: Option<Filter>,

    /// Ordering, outermost sort first.
    pub sort: Vec<SortField>,

    /// Maximum rows to return.
    pub limit: Option<u32>,

    /// Rows to skip.
    pub offset: Option<u32>,
}

impl QuerySpec {
    /// A query over a collection with no projection, predicate, or ordering.
    #[must_use]
    pub const fn new(collection: CollectionName) -> Self {
        Self {
            collection,
            fields: Vec::new(),
            filter: None,
            sort: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Restricts the projection.
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<FieldName>) -> Self {
        self.fields = fields;
        self
    }

    /// Applies a predicate.
    #[must_use]
    pub fn with_filter(mut self, filter: Filter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Applies an ordering.
    #[must_use]
    pub fn with_sort(mut self, sort: Vec<SortField>) -> Self {
        self.sort = sort;
        self
    }

    /// Applies pagination.
    #[must_use]
    pub const fn with_paging(mut self, limit: Option<u32>, offset: Option<u32>) -> Self {
        self.limit = limit;
        self.offset = offset;
        self
    }

    /// Returns the query as it must actually be executed for a given tenant.
    ///
    /// **Every path to a connector goes through here.** For a tenant using
    /// discriminator isolation (§18), this is what adds the predicate that
    /// makes the query see only that tenant's rows. Skip it and the query
    /// returns every tenant's data with no error raised — which is why the
    /// method exists at all rather than leaving callers to remember.
    ///
    /// For dedicated-database and per-tenant-schema placements there is no
    /// predicate to add, because isolation is enforced by the connection. In
    /// those cases this returns an equivalent query.
    ///
    /// # Schema qualification
    ///
    /// Note what this does *not* do: it does not rewrite the collection name to
    /// add a schema. Connectors name their collections in their own schema
    /// document, and a per-tenant schema is normally selected by the connection
    /// itself — a named connection per schema, or a `search_path` in a
    /// secret-derived connection string. The schema remains available on the
    /// target via [`IsolationModel::schema`](crate::IsolationModel::schema) for
    /// connector implementations that need it.
    #[must_use]
    pub fn for_target(&self, target: &ExecutionTarget) -> Self {
        let Some(tenant_predicate) = target.isolation().tenant_predicate() else {
            return self.clone();
        };

        let filter = match self.filter.clone() {
            Some(caller_filter) => caller_filter.and(tenant_predicate),
            None => tenant_predicate,
        };

        Self {
            filter: Some(filter),
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use fabric_core::{BindingRevision, TenantId};
    use serde_json::Value;

    use super::*;
    use crate::{ComparisonOperator, ConnectionSelector, ConnectorId, IsolationModel, SchemaName};

    fn collection() -> CollectionName {
        CollectionName::try_new("customers").unwrap()
    }

    fn target_with(isolation: IsolationModel) -> ExecutionTarget {
        ExecutionTarget::new(
            TenantId::try_new("acme").unwrap(),
            BindingRevision::new(1),
            ConnectorId::try_new("postgres").unwrap(),
            ConnectionSelector::Default,
            isolation,
        )
    }

    fn discriminator_target() -> ExecutionTarget {
        target_with(IsolationModel::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "tenant-482".to_owned(),
        })
    }

    fn caller_filter() -> Filter {
        Filter::Compare {
            field: FieldName::try_new("status").unwrap(),
            operator: ComparisonOperator::Equal,
            value: Value::String("active".to_owned()),
        }
    }

    #[test]
    fn a_dedicated_database_query_is_unchanged() {
        let spec = QuerySpec::new(collection()).with_filter(caller_filter());
        let executed = spec.for_target(&target_with(IsolationModel::Database));

        assert_eq!(executed, spec);
    }

    #[test]
    fn a_schema_isolated_query_is_unchanged_because_the_connection_isolates_it() {
        let spec = QuerySpec::new(collection());
        let executed = spec.for_target(&target_with(IsolationModel::Schema {
            schema: SchemaName::try_new("acme").unwrap(),
        }));

        assert_eq!(executed.filter, None);
        assert_eq!(executed.collection, collection());
    }

    #[test]
    fn a_discriminator_query_with_no_caller_filter_still_gets_the_tenant_predicate() {
        let executed = QuerySpec::new(collection()).for_target(&discriminator_target());

        // Without this, an unfiltered list returns every tenant's rows.
        assert_eq!(
            executed.filter,
            Some(discriminator_target().isolation().tenant_predicate().unwrap())
        );
    }

    #[test]
    fn a_discriminator_query_conjoins_the_tenant_predicate_with_the_caller_filter() {
        let executed = QuerySpec::new(collection())
            .with_filter(caller_filter())
            .for_target(&discriminator_target());

        let Some(Filter::And { clauses }) = executed.filter else {
            panic!("expected the caller filter to be conjoined with the tenant predicate");
        };

        assert_eq!(clauses.len(), 2);
        assert!(clauses.contains(&caller_filter()));
    }

    #[test]
    fn the_tenant_predicate_cannot_be_replaced_by_a_caller_filter_on_the_same_column() {
        // A caller filtering on the discriminator column must not be able to
        // widen its own scope: both predicates survive, so the conjunction can
        // only ever narrow.
        let hostile = Filter::Compare {
            field: FieldName::try_new("tenant_key").unwrap(),
            operator: ComparisonOperator::Equal,
            value: Value::String("tenant-999".to_owned()),
        };

        let executed = QuerySpec::new(collection())
            .with_filter(hostile.clone())
            .for_target(&discriminator_target());

        let Some(Filter::And { clauses }) = executed.filter else {
            panic!("expected a conjunction");
        };

        assert!(clauses.contains(&hostile));
        assert!(clauses.contains(&discriminator_target().isolation().tenant_predicate().unwrap()));
    }

    #[test]
    fn paging_and_projection_survive_targeting() {
        let spec = QuerySpec::new(collection())
            .with_fields(vec![FieldName::try_new("id").unwrap()])
            .with_paging(Some(10), Some(20));

        let executed = spec.for_target(&discriminator_target());

        assert_eq!(executed.limit, Some(10));
        assert_eq!(executed.offset, Some(20));
        assert_eq!(executed.fields.len(), 1);
    }
}
