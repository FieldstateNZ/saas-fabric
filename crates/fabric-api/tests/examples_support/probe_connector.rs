//! A connector whose health check is scripted, for readiness-probe tests.
//!
//! The readiness probe is the only thing in this workspace that calls
//! [`DataConnector::health`], and what it must do — answer concurrently, stop
//! waiting at a deadline, and never relay a backend's own words to an
//! unauthenticated caller — can only be exercised against connectors whose
//! health behaviour is chosen by the test.

use std::time::Duration;

use async_trait::async_trait;
use fabric_connector::{
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector, ExecutionTarget,
    MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};

/// What this connector's health check does when called.
#[derive(Clone)]
pub enum Health {
    /// Answers healthy at once.
    Healthy,
    /// Fails at once, carrying a message that names internal infrastructure.
    Failing(String),
    /// Sleeps for this long, then answers healthy — a blackholed backend.
    Slow(Duration),
}

/// A registered connector that executes nothing and reports scripted health.
pub struct ProbeConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    health: Health,
}

impl ProbeConnector {
    /// Registers under the given id with the given health behaviour.
    #[must_use]
    pub fn new(id: &str, health: Health) -> Self {
        Self {
            id: ConnectorId::try_new(id).expect("a valid connector id"),
            capabilities: ConnectorCapabilities::baseline(),
            schema: ConnectorSchema::default(),
            health,
        }
    }

    /// A connector that answers healthy immediately.
    #[must_use]
    pub fn healthy(id: &str) -> Self {
        Self::new(id, Health::Healthy)
    }
}

#[async_trait]
impl DataConnector for ProbeConnector {
    fn id(&self) -> &ConnectorId {
        &self.id
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    fn schema(&self) -> &ConnectorSchema {
        &self.schema
    }

    // This connector's schema is empty, so every collection is genuinely
    // unknown to it. Saying that is more honest than claiming a missing
    // capability, and it keeps the fixture out of `UnsupportedFeature`'s
    // vocabulary, which exists for capabilities a real backend can lack.
    async fn query(&self, _: &ExecutionTarget, spec: &QuerySpec) -> Result<QueryOutcome, ConnectorError> {
        Err(ConnectorError::UnknownCollection(spec.collection.clone()))
    }

    async fn mutate(
        &self,
        _: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        Err(ConnectorError::UnknownCollection(spec.collection().clone()))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        match &self.health {
            Health::Healthy => Ok(()),
            Health::Failing(message) => Err(ConnectorError::Rejected {
                connector: self.id.clone(),
                message: message.clone(),
            }),
            Health::Slow(delay) => {
                tokio::time::sleep(*delay).await;
                Ok(())
            }
        }
    }
}
