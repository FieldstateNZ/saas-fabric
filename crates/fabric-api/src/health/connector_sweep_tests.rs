//! The sweep's two properties: it runs concurrently, and it stops at the
//! budget — while still attributing every answer to the connector it came
//! from.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use fabric_connector::{
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorRegistry, ConnectorSchema, DataConnector,
    ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};

use super::connector_sweep::{sweep, HEALTH_BUDGET};

/// A connector whose health check sleeps and then answers as scripted.
struct ScriptedConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    delay: Duration,
    healthy: bool,
}

impl ScriptedConnector {
    fn registered(id: &str, delay: Duration, healthy: bool) -> Arc<dyn DataConnector> {
        Arc::new(Self {
            id: ConnectorId::try_new(id).unwrap(),
            capabilities: ConnectorCapabilities::baseline(),
            schema: ConnectorSchema::default(),
            delay,
            healthy,
        })
    }
}

#[async_trait]
impl DataConnector for ScriptedConnector {
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
        unreachable!("the sweep never executes an operation")
    }

    async fn mutate(&self, _: &ExecutionTarget, _: &MutationSpec) -> Result<MutationOutcome, ConnectorError> {
        unreachable!("the sweep never executes an operation")
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        tokio::time::sleep(self.delay).await;

        if self.healthy {
            Ok(())
        } else {
            Err(ConnectorError::Rejected {
                connector: self.id.clone(),
                message: format!("{} is refusing", self.id),
            })
        }
    }
}

fn registry(connectors: Vec<Arc<dyn DataConnector>>) -> ConnectorRegistry {
    connectors
        .into_iter()
        .fold(ConnectorRegistry::new(), ConnectorRegistry::with)
}

#[tokio::test]
async fn every_connector_is_checked_at_the_same_time() {
    let slow = Duration::from_millis(150);
    let connectors = registry(vec![
        ScriptedConnector::registered("a", slow, true),
        ScriptedConnector::registered("b", slow, true),
        ScriptedConnector::registered("c", slow, true),
    ]);

    let started = Instant::now();
    let outcomes = sweep(&connectors).await;
    let elapsed = started.elapsed();

    assert_eq!(outcomes.len(), 3);
    assert!(outcomes.iter().all(|outcome| outcome.health.is_healthy()));
    assert!(
        elapsed < slow * 2,
        "three concurrent 150ms checks took {elapsed:?}; that is a serial sweep"
    );
}

#[tokio::test]
async fn a_check_that_misses_the_budget_is_unknown_rather_than_unhealthy() {
    let connectors = registry(vec![
        ScriptedConnector::registered("fast", Duration::ZERO, true),
        ScriptedConnector::registered("blackholed", Duration::from_secs(10), true),
    ]);

    let started = Instant::now();
    let outcomes = sweep(&connectors).await;
    let elapsed = started.elapsed();

    assert!(
        elapsed < HEALTH_BUDGET * 2,
        "the sweep ran past its own budget: {elapsed:?}"
    );

    // Registry order is id order.
    let statuses: Vec<&str> = outcomes.iter().map(|outcome| outcome.health.status()).collect();
    assert_eq!(statuses, vec!["unknown", "healthy"]);
}

#[tokio::test]
async fn an_answer_is_attributed_to_the_connector_it_came_from() {
    // `slow-and-well` sorts first but finishes last. Matching answers to
    // connectors by arrival order rather than by task identity would report
    // the failure against it and pronounce the broken connector healthy.
    let connectors = registry(vec![
        ScriptedConnector::registered("slow-and-well", Duration::from_millis(80), true),
        ScriptedConnector::registered("swift-and-broken", Duration::ZERO, false),
    ]);

    let outcomes = sweep(&connectors).await;

    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].id, "slow-and-well");
    assert_eq!(outcomes[0].health.status(), "healthy");
    assert_eq!(outcomes[1].id, "swift-and-broken");
    assert_eq!(outcomes[1].health.status(), "unhealthy");
    assert!(outcomes[1]
        .health
        .reason()
        .unwrap()
        .contains("swift-and-broken is refusing"));
}

#[tokio::test]
async fn an_empty_registry_sweeps_to_nothing_without_waiting() {
    let started = Instant::now();
    let outcomes = sweep(&ConnectorRegistry::new()).await;

    assert!(outcomes.is_empty());
    assert!(started.elapsed() < HEALTH_BUDGET);
}
