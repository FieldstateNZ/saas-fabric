//! The [`DataConnector`] implementation that speaks NDC.

use std::sync::Arc;

use async_trait::async_trait;
use fabric_connector::{
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorSchema, DataConnector, ExecutionTarget,
    MutationOutcome, MutationSpec, QueryOutcome, QuerySpec, SecretResolver,
};

use crate::client::NdcHttpClient;
use crate::translate::{to_mutation_outcome, to_mutation_request, to_query_outcome, to_query_request};
use crate::{logging, routing, NdcConnectorConfig, SchemaIndex};

/// Executes neutral operations against an NDC connector service.
///
/// Built by [`build_ndc_connector`](crate::build_ndc_connector), which
/// negotiates capabilities and schema once at startup. Both are then held in
/// memory: `capabilities()` and `schema()` are called on every operation and
/// must not perform I/O.
pub struct NdcConnector {
    config: NdcConnectorConfig,
    client: NdcHttpClient,
    capabilities: ConnectorCapabilities,
    schema: SchemaIndex,
    secrets: Option<Arc<dyn SecretResolver>>,
}

impl NdcConnector {
    /// Assembles a negotiated connector.
    pub(crate) const fn new(
        config: NdcConnectorConfig,
        client: NdcHttpClient,
        capabilities: ConnectorCapabilities,
        schema: SchemaIndex,
        secrets: Option<Arc<dyn SecretResolver>>,
    ) -> Self {
        Self {
            config,
            client,
            capabilities,
            schema,
            secrets,
        }
    }
}

#[async_trait]
impl DataConnector for NdcConnector {
    fn id(&self) -> &ConnectorId {
        &self.config.id
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    fn schema(&self) -> &ConnectorSchema {
        self.schema.neutral()
    }

    async fn query(
        &self,
        target: &ExecutionTarget,
        spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        // Checked before translation so the common refusals produce a clear
        // capability error rather than an operator-lookup failure deep in the
        // predicate tree.
        self.capabilities
            .ensure_supports_query(spec)
            .inspect_err(|error| {
                logging::operation_refused(self.config.id.as_str(), "query", &error.to_string());
            })?;

        let arguments =
            routing::request_arguments(&self.config, target.connection(), self.secrets.as_ref()).await?;

        let request = to_query_request(spec, arguments, &self.schema).inspect_err(|error| {
            logging::operation_refused(self.config.id.as_str(), "query", &error.to_string());
        })?;

        let response = self.client.post("/query", &request).await.inspect_err(|error| {
            if let ConnectorError::Rejected { message, .. } = error {
                logging::connector_rejected(self.config.id.as_str(), "query", message);
            }
        })?;

        to_query_outcome(&self.config.id, &response)
    }

    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        self.capabilities
            .ensure_supports_mutation(spec)
            .inspect_err(|error| {
                logging::operation_refused(
                    self.config.id.as_str(),
                    spec.operation_name(),
                    &error.to_string(),
                );
            })?;

        let arguments =
            routing::request_arguments(&self.config, target.connection(), self.secrets.as_ref()).await?;

        let request =
            to_mutation_request(spec, arguments, &self.config, &self.schema).inspect_err(|error| {
                logging::operation_refused(
                    self.config.id.as_str(),
                    spec.operation_name(),
                    &error.to_string(),
                );
            })?;

        let response = self
            .client
            .post("/mutation", &request)
            .await
            .inspect_err(|error| {
                if let ConnectorError::Rejected { message, .. } = error {
                    logging::connector_rejected(self.config.id.as_str(), spec.operation_name(), message);
                }
            })?;

        to_mutation_outcome(&self.config.id, &response)
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        self.client.health().await
    }
}
