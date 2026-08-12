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
    /// # Errors
    ///
    /// Any [`DataApiError`], including [`DataApiError::BadRequest`] if `query`
    /// exceeds a configured complexity bound (§28).
    pub async fn list(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        query: &ListQuery,
    ) -> Result<ListResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::List)?;

        // Complexity, not authorization: checked once the caller is known to
        // be allowed here at all, and before anything is asked of a
        // connector.
        limits::enforce_query(query, &self.config)?;

        let limit = self.config.effective_limit(query.limit);
        let offset = query.offset.unwrap_or(0);

        let mut spec = QuerySpec::new(prepared.resource.collection.clone())
            .with_fields(query.select.clone())
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

        Ok(ListResponse::from_outcome(&outcome, limit, offset))
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

        let spec = QuerySpec::new(prepared.resource.collection.clone())
            .with_filter(key_filter(prepared.resource, key))
            .with_paging(Some(1), None);

        let outcome = self
            .execute_query(&prepared, &spec, resource_name, "read")
            .await?;

        outcome
            .rows
            .first()
            .map(RowResponse::from)
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

        Ok(prepared.connector.query(target, &scoped).await?)
    }
}
