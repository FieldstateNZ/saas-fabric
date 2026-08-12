//! `PendingConnector` fails closed until resolved, then delegates.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, ConnectionSelector, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema,
    DataConnector, ExecutionTarget, IsolationModel, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};
use fabric_core::{BindingRevision, DataSourceId, TenantId};

use super::pending_connector::PendingConnector;

/// A connector that always succeeds, so resolution can be observed.
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
        Ok(MutationOutcome::affected(1))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}

fn id(value: &str) -> ConnectorId {
    ConnectorId::try_new(value).unwrap()
}

fn target() -> ExecutionTarget {
    ExecutionTarget::new(
        TenantId::try_new("acme").unwrap(),
        BindingRevision::new(1),
        DataSourceId::try_new("shared-01").unwrap(),
        BindingRevision::new(1),
        id("postgres"),
        ConnectionSelector::Default,
        IsolationModel::Database,
    )
}

fn query() -> QuerySpec {
    QuerySpec::new(CollectionName::try_new("customers").unwrap())
}

fn insert() -> MutationSpec {
    MutationSpec::Insert {
        collection: CollectionName::try_new("customers").unwrap(),
        rows: vec![],
    }
}

#[tokio::test]
async fn health_fails_closed_with_the_startup_reason_before_resolution() {
    let pending = PendingConnector::new(id("postgres"), "connection refused".to_owned());

    let Err(error) = pending.health().await else {
        panic!("an unresolved connector must not report healthy");
    };

    let ConnectorError::Unreachable { connector, source } = error else {
        panic!("expected Unreachable, got {error:?}");
    };

    assert_eq!(connector, id("postgres"));
    assert!(source.to_string().contains("connection refused"));
}

#[tokio::test]
async fn query_and_mutate_also_fail_closed_before_resolution() {
    let pending = PendingConnector::new(id("postgres"), "timed out".to_owned());

    assert!(pending.query(&target(), &query()).await.is_err());
    assert!(pending.mutate(&target(), &insert()).await.is_err());
}

#[tokio::test]
async fn a_later_failure_reason_replaces_the_startup_one() {
    let pending = PendingConnector::new(id("postgres"), "startup failure".to_owned());
    pending.record_failure("retry failure".to_owned());

    let Err(ConnectorError::Unreachable { source, .. }) = pending.health().await else {
        panic!("still expected Unreachable before resolution");
    };

    assert!(source.to_string().contains("retry failure"));
    assert!(!source.to_string().contains("startup failure"));
}

#[tokio::test]
async fn resolving_makes_health_delegate_to_the_real_connector() {
    let pending = PendingConnector::new(id("postgres"), "connection refused".to_owned());

    pending.resolve(Arc::new(StubConnector::new("postgres")));

    assert!(pending.health().await.is_ok());
}

#[tokio::test]
async fn resolving_makes_query_and_mutate_delegate_to_the_real_connector() {
    let pending = PendingConnector::new(id("postgres"), "connection refused".to_owned());
    pending.resolve(Arc::new(StubConnector::new("postgres")));

    assert!(pending.query(&target(), &query()).await.is_ok());

    let outcome = pending.mutate(&target(), &insert()).await.unwrap();

    assert_eq!(outcome, MutationOutcome::affected(1));
}

#[test]
fn id_reports_the_configured_connector_id_regardless_of_resolution() {
    let pending = PendingConnector::new(id("postgres"), "connection refused".to_owned());

    assert_eq!(pending.id(), &id("postgres"));
}
