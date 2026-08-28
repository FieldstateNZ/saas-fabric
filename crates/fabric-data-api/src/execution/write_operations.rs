//! Writes.

use fabric_connector::MutationSpec;
use fabric_core::LogicalResourceName;
use fabric_identity::TenantIdentity;
use serde_json::{Map, Value};

use crate::execution::dispatch_write::dispatch;
use crate::execution::row_mapping::{key_filter, to_row};
use crate::execution::write_integrity::RowBudget;
use crate::{limits, DataApiError, DataApiService, OperationKind, WriteResponse};

impl DataApiService {
    /// Creates records.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`], including [`DataApiError::BadRequest`] if `rows`
    /// exceeds the configured maximum batch size (§28), and
    /// [`DataApiError::PartiallyApplied`] if the backend reports writing fewer
    /// records than were sent.
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

        let writable = prepared.writable_fields();
        let rows = rows
            .iter()
            .map(|row| to_row(row, &writable))
            .collect::<Result<Vec<_>, _>>()?;

        // Captured before the spec takes ownership, and the only number the
        // platform can check the backend's answer against.
        let budget = RowBudget::Batch(u64::try_from(rows.len()).unwrap_or(u64::MAX));

        let spec = MutationSpec::Insert {
            collection: prepared.resource.collection.clone(),
            rows,
        };

        dispatch(&prepared, &spec, resource_name, &budget).await
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
            changes: to_row(changes, &prepared.writable_fields())?,
        };

        dispatch(&prepared, &spec, resource_name, &RowBudget::OneRecord).await
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

        dispatch(&prepared, &spec, resource_name, &RowBudget::OneRecord).await
    }
}
