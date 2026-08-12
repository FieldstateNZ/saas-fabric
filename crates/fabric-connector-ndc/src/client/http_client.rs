//! The HTTP client itself.

use std::time::Duration;

use fabric_connector::{ConnectorError, ConnectorId};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::client::error_mapping::unreachable;
use crate::client::response_decoding::decode;
use crate::{NdcConnectorConfig, NDC_VERSION, NDC_VERSION_HEADER};

/// Talks to one NDC connector over HTTP.
///
/// # Connection management
///
/// This holds a `reqwest::Client`, which owns a keep-alive connection pool to
/// the connector. That is the pool the runtime plane manages now — the
/// *database* pool lives inside the connector process, which is a consequence
/// of adopting NDC that ADR 0001 records explicitly.
///
/// The §22 objective still holds, and arguably holds better: database
/// connections are concentrated in a small number of connector processes rather
/// than multiplied across every application replica. What changed is that the
/// sizing and eviction knobs are the connector's configuration, declared on the
/// [`DataSource`](fabric_tenant_runtime::DataSource) and applied by
/// reconciliation.
pub(crate) struct NdcHttpClient {
    http: reqwest::Client,
    endpoint: String,
    connector: ConnectorId,
}

impl NdcHttpClient {
    /// Builds a client from configuration.
    ///
    /// Applies both HTTP timeouts from [`NdcConnectorConfig`]: `.timeout(..)`
    /// bounds the whole call, `.connect_timeout(..)` bounds the connect phase
    /// specifically. Neither one reaches into the connector's own database
    /// timeout or the host's overall request budget — see
    /// [`NdcConnectorConfig::http_timeout_seconds`] for why those are
    /// deliberately owned elsewhere.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be constructed.
    pub(crate) fn new(config: &NdcConnectorConfig) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.http_timeout_seconds))
            .connect_timeout(Duration::from_secs(config.http_connect_timeout_seconds))
            .build()
            .map_err(|error| format!("could not build an HTTP client for {}: {error}", config.id))?;

        Ok(Self {
            http,
            endpoint: config.endpoint.trim_end_matches('/').to_owned(),
            connector: config.id.clone(),
        })
    }

    /// Issues a `GET` and decodes the response.
    ///
    /// # Errors
    ///
    /// Any [`ConnectorError`] arising from transport, status, or decoding.
    pub(crate) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ConnectorError> {
        let response = self
            .request(reqwest::Method::GET, path)
            .send()
            .await
            .map_err(|error| unreachable(&self.connector, error))?;

        decode(&self.connector, response).await
    }

    /// Issues a `POST` with a JSON body and decodes the response.
    ///
    /// # Errors
    ///
    /// Any [`ConnectorError`] arising from transport, status, or decoding.
    pub(crate) async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ConnectorError> {
        let response = self
            .request(reqwest::Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(|error| unreachable(&self.connector, error))?;

        decode(&self.connector, response).await
    }

    /// Checks that the connector answers its health endpoint.
    ///
    /// # Errors
    ///
    /// [`ConnectorError::Unreachable`] or [`ConnectorError::Rejected`].
    pub(crate) async fn health(&self) -> Result<(), ConnectorError> {
        let response = self
            .request(reqwest::Method::GET, "/health")
            .send()
            .await
            .map_err(|error| unreachable(&self.connector, error))?;

        if response.status().is_success() {
            return Ok(());
        }

        Err(ConnectorError::Rejected {
            connector: self.connector.clone(),
            message: format!("health check returned {}", response.status()),
        })
    }

    /// Starts a request carrying the negotiated protocol version.
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.endpoint))
            .header(NDC_VERSION_HEADER, NDC_VERSION)
    }
}
