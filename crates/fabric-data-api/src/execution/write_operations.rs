//! Writes.

use fabric_connector::MutationSpec;
use fabric_core::LogicalResourceName;
use fabric_identity::TenantIdentity;
use serde_json::{Map, Value};

use crate::execution::prepared::Prepared;
use crate::execution::row_mapping::{key_filter, to_row};
use crate::{limits, logging, DataApiError, DataApiService, OperationKind, WriteResponse};

impl DataApiService {
    /// Creates records.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`], including [`DataApiError::BadRequest`] if `rows`
    /// exceeds the configured maximum batch size (§28).
    pub async fn create(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        rows: Vec<Map<String, Value>>,
    ) -> Result<WriteResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::Create)?;

        if rows.is_empty() {
            return Err(DataApiError::BadRequest("no records to create".to_owned()));
        }

        limits::enforce_batch_size(rows.len(), &self.config)?;

        let spec = MutationSpec::Insert {
            collection: prepared.resource.collection.clone(),
            rows: rows
                .iter()
                .map(|row| to_row(row, prepared.resource))
                .collect::<Result<Vec<_>, _>>()?,
        };

        self.execute_mutation(&prepared, &spec, resource_name).await
    }

    /// Updates one record by key.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`].
    pub async fn update(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        key: &str,
        changes: &Map<String, Value>,
    ) -> Result<WriteResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::Update)?;

        if changes.is_empty() {
            return Err(DataApiError::BadRequest("no fields to update".to_owned()));
        }

        let spec = MutationSpec::Update {
            collection: prepared.resource.collection.clone(),
            filter: Some(key_filter(prepared.resource, key)),
            changes: to_row(changes, prepared.resource)?,
        };

        self.execute_mutation(&prepared, &spec, resource_name).await
    }

    /// Deletes one record by key.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`].
    pub async fn delete(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        key: &str,
    ) -> Result<WriteResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::Delete)?;

        // Always keyed. The Data API exposes no unfiltered delete: a caller
        // cannot ask to empty a collection, whatever their scopes.
        let spec = MutationSpec::Delete {
            collection: prepared.resource.collection.clone(),
            filter: Some(key_filter(prepared.resource, key)),
        };

        self.execute_mutation(&prepared, &spec, resource_name).await
    }

    /// Applies tenant scoping and dispatches a write.
    async fn execute_mutation(
        &self,
        prepared: &Prepared<'_>,
        spec: &MutationSpec,
        resource_name: &LogicalResourceName,
    ) -> Result<WriteResponse, DataApiError> {
        let target = &prepared.resolved.target;

        logging::operation_dispatched(
            resource_name,
            &prepared.resource.data_source,
            spec.operation_name(),
            target,
        );

        // `for_target` scopes the predicate and stamps the tenant discriminator
        // onto written rows. Every write goes through it.
        let scoped = spec.for_target(target);

        let outcome = prepared.connector.mutate(target, &scoped).await?;

        Ok(WriteResponse::from_outcome(&outcome))
    }
}
