//! What a pending connector does when someone tries to use it.
//!
//! Every operation asks the same question — has negotiation succeeded yet? —
//! and either forwards to the real connector or refuses. Kept apart from the
//! type's own state so that the delegation stays visibly uniform: if one of
//! these three ever stops matching the others, it should be obvious at a
//! glance rather than buried among constructors and lock helpers.

use async_trait::async_trait;
use fabric_connector::{
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector, ExecutionTarget,
    MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};

use crate::startup::connectors::pending_connector::PendingConnector;

#[async_trait]
impl DataConnector for PendingConnector {
    fn id(&self) -> &ConnectorId {
        self.id()
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        self.capabilities()
    }

    fn schema(&self) -> &ConnectorSchema {
        self.schema()
    }

    async fn query(
        &self,
        target: &ExecutionTarget,
        spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        match self.resolved_connector() {
            Some(connector) => connector.query(target, spec).await,
            None => Err(self.unavailable()),
        }
    }

    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        match self.resolved_connector() {
            Some(connector) => connector.mutate(target, spec).await,
            None => Err(self.unavailable()),
        }
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        match self.resolved_connector() {
            Some(connector) => connector.health().await,
            None => Err(self.unavailable()),
        }
    }
}
