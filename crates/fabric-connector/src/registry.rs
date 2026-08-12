//! The set of connectors this process can execute against.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{ConnectorError, ConnectorId, DataConnector};

/// The connectors available to the runtime, by id.
///
/// Built once at startup from configuration and then read-only. A tenant's
/// runtime binding names a connector; this is what turns that name into
/// something executable.
///
/// # Why this is fixed at startup
///
/// Connector *instances* are process-level infrastructure — an HTTP client, a
/// cached schema, a cached capability set. Tenants come and go through binding
/// refresh without touching this map, because many tenants share one connector.
/// That separation is what stops connection counts scaling with tenant count
/// (§22): adding a tenant to a shared database adds no connector and no pool.
///
/// A genuinely new physical backend is a deployment change, and deployment
/// changes go through reconciliation (§5), not through the request path.
#[derive(Clone, Default)]
pub struct ConnectorRegistry {
    connectors: BTreeMap<ConnectorId, Arc<dyn DataConnector>>,
}

impl ConnectorRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connectors: BTreeMap::new(),
        }
    }

    /// Adds a connector, returning the registry for chaining.
    ///
    /// A second connector with the same id replaces the first. That is a
    /// configuration mistake rather than something to support, so it is worth
    /// noticing in review — but failing at startup would be worse than the last
    /// one winning deterministically.
    #[must_use]
    pub fn with(mut self, connector: Arc<dyn DataConnector>) -> Self {
        self.connectors.insert(connector.id().clone(), connector);
        self
    }

    /// Looks up a connector.
    ///
    /// # Errors
    ///
    /// [`ConnectorError::UnknownConnector`] when nothing is registered under
    /// the id. This is a fail-closed path: a binding naming a connector that is
    /// not deployed must reject the request, never fall back to another
    /// connector (§28).
    pub fn get(&self, id: &ConnectorId) -> Result<&Arc<dyn DataConnector>, ConnectorError> {
        self.connectors
            .get(id)
            .ok_or_else(|| ConnectorError::UnknownConnector(id.clone()))
    }

    /// Every registered connector, for health checks and startup logging.
    pub fn all(&self) -> impl Iterator<Item = &Arc<dyn DataConnector>> {
        self.connectors.values()
    }

    /// How many connectors are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.connectors.len()
    }

    /// Whether no connectors are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.connectors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{
        ConnectorCapabilities, ConnectorSchema, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome,
        QuerySpec,
    };

    struct StubConnector {
        id: ConnectorId,
        capabilities: ConnectorCapabilities,
        schema: ConnectorSchema,
    }

    impl StubConnector {
        fn new(id: &str) -> Self {
            Self {
                id: ConnectorId::try_new(id).unwrap(),
                capabilities: ConnectorCapabilities::baseline(),
                schema: ConnectorSchema::default(),
            }
        }
    }

    #[async_trait]
    impl DataConnector for StubConnector {
        fn id(&self) -> &ConnectorId {
            &self.id
        }

        fn capabilities(&self) -> &ConnectorCapabilities {
            &self.capabilities
        }

        fn schema(&self) -> &ConnectorSchema {
            &self.schema
        }

        async fn query(&self, _: &ExecutionTarget, _: &QuerySpec) -> Result<QueryOutcome, ConnectorError> {
            Ok(QueryOutcome::default())
        }

        async fn mutate(
            &self,
            _: &ExecutionTarget,
            _: &MutationSpec,
        ) -> Result<MutationOutcome, ConnectorError> {
            Ok(MutationOutcome::affected(0))
        }

        async fn health(&self) -> Result<(), ConnectorError> {
            Ok(())
        }
    }

    #[test]
    fn resolves_a_registered_connector() {
        let registry = ConnectorRegistry::new().with(Arc::new(StubConnector::new("postgres")));

        assert!(registry.get(&ConnectorId::try_new("postgres").unwrap()).is_ok());
    }

    #[test]
    fn an_unregistered_connector_fails_closed_rather_than_falling_back() {
        let registry = ConnectorRegistry::new().with(Arc::new(StubConnector::new("postgres")));

        // Even though exactly one connector exists, asking for a different one
        // must not silently use it.
        let Err(error) = registry.get(&ConnectorId::try_new("sqlserver").unwrap()) else {
            panic!("an unregistered connector must not resolve");
        };

        assert!(matches!(error, ConnectorError::UnknownConnector(_)));
    }
}
