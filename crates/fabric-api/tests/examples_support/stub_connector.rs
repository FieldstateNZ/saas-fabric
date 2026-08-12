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
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector, ExecutionTarget,
    MutationOutcome, MutationSpec, QueryOutcome, QuerySpec,
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
    fn refused() -> ConnectorError {
        ConnectorError::Unsupported {
            feature: "this stub executes nothing".to_owned(),
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
        Err(Self::refused())
    }

    async fn mutate(&self, _: &ExecutionTarget, _: &MutationSpec) -> Result<MutationOutcome, ConnectorError> {
        Err(Self::refused())
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Err(Self::refused())
    }
}
