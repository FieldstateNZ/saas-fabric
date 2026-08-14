//! Reads.

use fabric_connector::{QueryOutcome, QuerySpec};
use fabric_core::LogicalResourceName;
use fabric_identity::TenantIdentity;

use crate::execution::prepared::Prepared;
use crate::execution::row_mapping::key_filter;
use crate::{
    limits, logging, DataApiError, DataApiService, ListQuery, ListResponse, OperationKind, RowResponse,
};

impl DataApiService {
    /// Lists records.
    ///
    /// # Why this takes the raw query string
    ///
    /// Parsing a list query checks every field name against the resource's
    /// `queryable_fields`, so a parse is a statement about the resource. Doing
    /// it before [`Self::prepare`] would let a caller with no scopes tell a
    /// real field from an invented one by watching 400 against 403 — which is
    /// exactly the ordering `prepare`'s own rustdoc forbids. The handler
    /// therefore hands the string over untouched and the parse happens here,
    /// after authorization has already decided the answer.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`], including [`DataApiError::BadRequest`] for a field
    /// the resource does not expose or a query that exceeds a configured
    /// complexity bound (§28).
    pub async fn list(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        raw_query: &str,
    ) -> Result<ListResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::List)?;

        // Shape, then complexity. Neither is authorization, so both wait until
        // the caller is known to be allowed here at all — and both finish
        // before anything is asked of a connector.
        let query = ListQuery::parse(raw_query, prepared.resource)?;
        limits::enforce_query(&query, &self.config)?;

        let limit = self.config.effective_limit(query.limit);
        let offset = query.offset.unwrap_or(0);

        // `projection` rather than `query.select` directly: an absent `select`
        // used to leave the field list empty, which the connector contract
        // reads as "give me everything". On a resource with an allowlist that
        // asked the backend for the very columns the resource hides.
        let mut spec = QuerySpec::new(prepared.resource.collection.clone())
            .with_fields(prepared.resource.projection(&query.select))
            .with_sort(query.sort.clone())
            // One row beyond the page, so `has_more` is a fact rather than a
            // guess. The extra row is trimmed before the response is built.
            .with_paging(Some(limit.saturating_add(1)), Some(offset));

        if let Some(filter) = query.to_filter() {
            spec = spec.with_filter(filter);
        }

        let outcome = self
            .execute_query(&prepared, &spec, resource_name, "list")
            .await?;

        Ok(ListResponse::from_outcome(
            &outcome,
            &prepared.visible_fields(),
            limit,
            offset,
        ))
    }

    /// Reads one record by key.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`], including [`DataApiError::NotFound`].
    pub async fn read(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        key: &str,
    ) -> Result<RowResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::Read)?;

        // There is no `select` on this route, so the projection is entirely the
        // resource's. It used to be absent altogether, which meant a read by
        // key could not restrict its columns even in principle.
        let spec = QuerySpec::new(prepared.resource.collection.clone())
            .with_fields(prepared.resource.projection(&[]))
            .with_filter(key_filter(prepared.resource, key))
            .with_paging(Some(1), None);

        let outcome = self
            .execute_query(&prepared, &spec, resource_name, "read")
            .await?;

        outcome
            .rows
            .first()
            .map(|row| RowResponse::project(row, &prepared.visible_fields()))
            .ok_or(DataApiError::NotFound)
    }

    /// Applies tenant scoping and dispatches a read.
    pub(super) async fn execute_query(
        &self,
        prepared: &Prepared<'_>,
        spec: &QuerySpec,
        resource_name: &LogicalResourceName,
        operation: &str,
    ) -> Result<QueryOutcome, DataApiError> {
        let target = &prepared.resolved.target;

        logging::operation_dispatched(resource_name, &prepared.resource.data_source, operation, target);

        // `for_target` is what adds the tenant predicate under discriminator
        // isolation. Every read goes through it.
        let scoped = spec.for_target(target);

        prepared
            .connector
            .query(target, &scoped)
            .await
            .map_err(|error| prepared.failed(error))
    }
}
