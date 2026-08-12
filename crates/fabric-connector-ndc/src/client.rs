//! The HTTP transport to one connector service.

use std::time::Duration;

use fabric_connector::{ConnectorError, ConnectorId};
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::wire::NdcErrorResponse;
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
/// sizing and eviction knobs are the connector's configuration rather than our
/// code.
pub(crate) struct NdcHttpClient {
    http: reqwest::Client,
    endpoint: String,
    connector: ConnectorId,
}

impl NdcHttpClient {
    /// Builds a client from configuration.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be constructed.
    pub(crate) fn new(config: &NdcConnectorConfig) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
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
            .http
            .get(format!("{}{path}", self.endpoint))
            .header(NDC_VERSION_HEADER, NDC_VERSION)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;

        self.decode(response).await
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
            .http
            .post(format!("{}{path}", self.endpoint))
            .header(NDC_VERSION_HEADER, NDC_VERSION)
            .json(body)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;

        self.decode(response).await
    }

    /// Checks that the connector answers its health endpoint.
    ///
    /// # Errors
    ///
    /// [`ConnectorError::Unreachable`] or [`ConnectorError::Rejected`].
    pub(crate) async fn health(&self) -> Result<(), ConnectorError> {
        let response = self
            .http
            .get(format!("{}/health", self.endpoint))
            .header(NDC_VERSION_HEADER, NDC_VERSION)
            .send()
            .await
            .map_err(|error| self.unreachable(error))?;

        if response.status().is_success() {
            return Ok(());
        }

        Err(ConnectorError::Rejected {
            connector: self.connector.clone(),
            message: format!("health check returned {}", response.status()),
        })
    }

    /// Turns a response into a decoded value, or the right error.
    async fn decode<T: DeserializeOwned>(&self, response: reqwest::Response) -> Result<T, ConnectorError> {
        let status = response.status();

        let body = response.bytes().await.map_err(|error| self.unreachable(error))?;

        if !status.is_success() {
            return Err(self.rejected(status, &body));
        }

        serde_json::from_slice(&body).map_err(|error| ConnectorError::MalformedResponse {
            connector: self.connector.clone(),
            detail: error.to_string(),
        })
    }

    /// Builds a rejection error, preferring the connector's own message.
    ///
    /// The message is kept for the log. It must not be returned to an
    /// application: connector errors name physical tables, schemas, and servers,
    /// which §2 and §29 keep internal. The Data API is responsible for that
    /// last step, and [`ConnectorError::is_internal`] tells it which errors to
    /// replace with a generic message.
    fn rejected(&self, status: reqwest::StatusCode, body: &[u8]) -> ConnectorError {
        let message = serde_json::from_slice::<NdcErrorResponse>(body)
            .map_or_else(|_| format!("connector returned {status}"), |error| error.message);

        ConnectorError::Rejected {
            connector: self.connector.clone(),
            message,
        }
    }

    /// Builds a transport error.
    fn unreachable(&self, error: reqwest::Error) -> ConnectorError {
        ConnectorError::Unreachable {
            connector: self.connector.clone(),
            source: Box::new(error),
        }
    }
}
