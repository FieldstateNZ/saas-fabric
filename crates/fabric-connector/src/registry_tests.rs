//! Tests for registry.

use super::registry::*;
use crate::{
    ConnectorCapabilities, ConnectorSchema, ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome,
    QuerySpec,
};
use crate::{ConnectorError, ConnectorId, DataConnector};
use async_trait::async_trait;
use std::sync::Arc;

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

    async fn mutate(&self, _: &ExecutionTarget, _: &MutationSpec) -> Result<MutationOutcome, ConnectorError> {
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
