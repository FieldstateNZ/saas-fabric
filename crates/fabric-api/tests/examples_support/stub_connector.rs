//! A connector that satisfies registration and executes nothing.
//!
//! `build_data_api` refuses an empty connector registry — correctly, since a
//! Data API with nowhere to execute can serve no request. Routing tests still
//! need to get past that check without standing up an NDC process, so this
//! registers under the example configuration's connector id and fails every
//! operation. Nothing that reaches it should be reaching it.

#![allow(dead_code)]

use async_trait::async_trait;
use fabric_connector::{
    CollectionName, ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector,
    ExecutionTarget, MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
};

/// A connector that is registered but never usable.
pub struct StubConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
}

impl StubConnector {
    /// Registers under the given id, supporting nothing.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            id: ConnectorId::try_new(id).expect("a valid connector id"),
            capabilities: ConnectorCapabilities::baseline(),
            schema: ConnectorSchema::default(),
        }
    }

    /// The error every operation returns.
    ///
    /// The stub's schema is empty, so every collection really is unknown to
    /// it — a truer answer than borrowing a capability name, and it leaves
    /// `UnsupportedFeature` to mean what a real backend cannot do.
    fn refused(collection: &CollectionName) -> ConnectorError {
        ConnectorError::UnknownCollection(collection.clone())
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

    async fn query(&self, _: &ExecutionTarget, spec: &QuerySpec) -> Result<QueryOutcome, ConnectorError> {
        Err(Self::refused(&spec.collection))
    }

    async fn mutate(
        &self,
        _: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        Err(Self::refused(spec.collection()))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        // Not collection-shaped, and internal, so the readiness probe masks it
        // — which is the behaviour the composed-surface test relies on.
        Err(ConnectorError::Rejected {
            connector: self.id.clone(),
            status: 503,
            message: "this stub executes nothing".to_owned(),
        })
    }
}
