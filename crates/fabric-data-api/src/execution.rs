//! The resolution chain, from an authenticated identity to executed work.

use std::sync::Arc;

use fabric_connector::{
    ComparisonOperator, ConnectorRegistry, DataConnector, ExecutionTarget, Filter, MutationSpec, QuerySpec,
    Row,
};
use fabric_core::LogicalResourceName;
use fabric_identity::TenantIdentity;
use fabric_tenant_runtime::TenantRuntimeRegistry;
use serde_json::{Map, Value};

use crate::{
    logging, DataApiConfig, DataApiError, ListQuery, ListResponse, OperationKind, ResourceCatalog,
    ResourceDefinition, ResourcePermissions, RowResponse, WriteResponse,
};

/// Executes Data API operations.
///
/// This is where the platform's core promise is actually kept:
///
/// ```text
/// bearer token → tenant_id → TenantRuntimeBinding → logical data source
///              → ExecutionTarget → connector → physical resource
/// ```
///
/// Every method walks that chain through [`Self::prepare`], and every one of
/// them applies `for_target` before dispatching. Nothing here takes a tenant as
/// a parameter — the only source is the [`TenantIdentity`], which came from the
/// bearer token (§10, §11).
pub struct DataApiService {
    tenants: Arc<TenantRuntimeRegistry>,
    connectors: ConnectorRegistry,
    catalog: ResourceCatalog,
    permissions: ResourcePermissions,
    config: DataApiConfig,
}

/// Everything one operation needs, once resolution has succeeded.
struct Prepared<'a> {
    resource: &'a ResourceDefinition,
    target: ExecutionTarget,
    connector: Arc<dyn DataConnector>,
}

impl DataApiService {
    /// Builds the service. Called from [`build_data_api`](crate::build_data_api).
    #[must_use]
    pub const fn new(
        tenants: Arc<TenantRuntimeRegistry>,
        connectors: ConnectorRegistry,
        catalog: ResourceCatalog,
        permissions: ResourcePermissions,
        config: DataApiConfig,
    ) -> Self {
        Self {
            tenants,
            connectors,
            catalog,
            permissions,
            config,
        }
    }

    /// The catalogue, for the discovery endpoint.
    #[must_use]
    pub const fn catalog(&self) -> &ResourceCatalog {
        &self.catalog
    }

    /// The configured limits.
    #[must_use]
    pub const fn config(&self) -> &DataApiConfig {
        &self.config
    }

    /// Lists records.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`].
    pub async fn list(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        query: &ListQuery,
    ) -> Result<ListResponse, DataApiError> {
        let prepared = self.prepare(identity, resource_name, OperationKind::List)?;

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

    /// Creates records.
    ///
    /// # Errors
    ///
    /// Any [`DataApiError`].
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

    /// Walks the resolution chain and authorises the operation.
    ///
    /// Order matters. The resource is resolved first so that a request for a
    /// resource that does not exist gets a 404 regardless of scopes; then
    /// authorization; then the tenant's runtime binding. Doing authorization
    /// before touching the registry means an unauthorised caller cannot use
    /// timing or status codes to learn anything about the tenant estate.
    fn prepare(
        &self,
        identity: &TenantIdentity,
        resource_name: &LogicalResourceName,
        operation: OperationKind,
    ) -> Result<Prepared<'_>, DataApiError> {
        let resource = self.catalog.resolve(resource_name)?;

        if !resource.allows(operation) {
            return Err(DataApiError::OperationNotAllowed {
                resource: resource_name.to_string(),
                operation: operation.as_str(),
            });
        }

        if !self
            .permissions
            .permits(identity, operation, resource_name.as_str())
        {
            logging::operation_forbidden(resource_name.as_str(), operation.as_str(), identity.subject());

            return Err(DataApiError::Forbidden {
                resource: resource_name.to_string(),
                operation: operation.as_str(),
            });
        }

        // The tenant comes from the identity and nowhere else (§10, §11).
        let binding = self.tenants.resolve(identity.tenant())?;
        let target = binding.execution_target(&resource.data_source)?;
        let connector = Arc::clone(self.connectors.get(target.connector())?);

        Ok(Prepared {
            resource,
            target,
            connector,
        })
    }

    /// Applies tenant scoping and dispatches a read.
    async fn execute_query(
        &self,
        prepared: &Prepared<'_>,
        spec: &QuerySpec,
        resource_name: &LogicalResourceName,
        operation: &str,
    ) -> Result<fabric_connector::QueryOutcome, DataApiError> {
        logging::operation_dispatched(resource_name, operation, &prepared.target);

        // `for_target` is what adds the tenant predicate under discriminator
        // isolation. Every read goes through it.
        let scoped = spec.for_target(&prepared.target);

        Ok(prepared.connector.query(&prepared.target, &scoped).await?)
    }

    /// Applies tenant scoping and dispatches a write.
    async fn execute_mutation(
        &self,
        prepared: &Prepared<'_>,
        spec: &MutationSpec,
        resource_name: &LogicalResourceName,
    ) -> Result<WriteResponse, DataApiError> {
        logging::operation_dispatched(resource_name, spec.operation_name(), &prepared.target);

        // `for_target` scopes the predicate and stamps the discriminator onto
        // written rows. Every write goes through it.
        let scoped = spec.for_target(&prepared.target);

        let outcome = prepared.connector.mutate(&prepared.target, &scoped).await?;

        Ok(WriteResponse::from_outcome(&outcome))
    }
}

/// Builds the predicate selecting one record by its key.
fn key_filter(resource: &ResourceDefinition, key: &str) -> Filter {
    Filter::Compare {
        field: resource.key_field.clone(),
        operator: ComparisonOperator::Equal,
        value: Value::String(key.to_owned()),
    }
}

/// Converts a JSON object into a neutral row, validating field names.
fn to_row(object: &Map<String, Value>, resource: &ResourceDefinition) -> Result<Row, DataApiError> {
    let mut row = Row::new();

    for (name, value) in object {
        let field = fabric_connector::FieldName::try_new(name)
            .map_err(|error| DataApiError::BadRequest(format!("invalid field name: {error}")))?;

        if !resource.permits_field(&field) {
            return Err(DataApiError::BadRequest(format!("unknown field {field}")));
        }

        row = row.with(field, value.clone());
    }

    Ok(row)
}
