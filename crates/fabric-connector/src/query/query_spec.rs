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

    /// Fields to return.
    ///
    /// **Empty is not a default — it is the absence of a constraint.** It asks
    /// the connector for no particular projection, and a backend answering an
    /// unprojected read returns every column the collection has, including ones
    /// the caller must never see: another resource's columns, and on a shared
    /// table the tenant discriminator itself.
    ///
    /// A caller that must limit what comes back therefore has to populate this,
    /// and must *also* filter the rows it gets: nothing here obliges a connector
    /// to honour a projection, so an empty response-side check is a control that
    /// only appears to work. `fabric-data-api` does both —
    /// `ResourceDefinition::projection` fills this in from the resource's
    /// `queryable_fields`, and `RowResponse::project` drops anything that comes
    /// back regardless.
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
    /// secret-derived connection string.
    ///
    /// That is the whole story today, not half of it:
    /// [`IsolationModel::Schema`](crate::IsolationModel::Schema) is a deferred
    /// capability (ADR 0006) and no connector in this workspace reads the schema
    /// off a target. Nothing here is waiting for a connector to opt in.
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
