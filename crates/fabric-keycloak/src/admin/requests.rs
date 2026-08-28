//! The four operations the adapter performs.
//!
//! Split from the client's own file because these are two concerns that share a
//! struct: `http` owns how a request is made — the client, the token, the
//! bearer header — and this owns what the operations are and how each status is
//! read. The house convention for that split is an impl block in its own
//! module.

use fabric_reconciliation::ProviderError;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::admin::errors::{status_failure, transport_failure};
use crate::admin::http::KeycloakAdmin;

impl KeycloakAdmin {
    /// Reads a resource, treating `404` as "it is not there".
    ///
    /// The distinction is the reconciler's entire first branch: a realm that
    /// does not exist has to be created, while a realm that could not be read
    /// must not be, because creating one over a realm that is merely
    /// unreachable is how a live realm gets replaced by an empty one.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for anything that is not a success or a
    /// `404`.
    pub(crate) async fn get_optional<T: DeserializeOwned>(
        &self,
        operation: &str,
        url: String,
    ) -> Result<Option<T>, ProviderError> {
        let response = self.send(operation, self.http.get(url)).await?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        Self::decode(operation, response).await.map(Some)
    }

    /// Reads a resource.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for any non-success status.
    pub(crate) async fn get<T: DeserializeOwned>(
        &self,
        operation: &str,
        url: String,
    ) -> Result<T, ProviderError> {
        let response = self.send(operation, self.http.get(url)).await?;

        Self::decode(operation, response).await
    }

    /// Creates a resource, treating `409` as success.
    ///
    /// The port requires every create to be idempotent, and this is where that
    /// is made true: Keycloak answers `409 Conflict` for something that
    /// already exists, and a reconciler racing its own previous pass — or
    /// Keycloak having created a role for itself — must not turn that into a
    /// failed sweep.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for any other non-success status.
    pub(crate) async fn create<B: Serialize + Sync>(
        &self,
        operation: &str,
        url: String,
        body: &B,
    ) -> Result<(), ProviderError> {
        let response = self.send(operation, self.http.post(url).json(body)).await?;

        if response.status() == StatusCode::CONFLICT {
            return Ok(());
        }

        Self::expect_success(operation, &response)
    }

    /// Replaces a resource.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for any non-success status.
    pub(crate) async fn update<B: Serialize + Sync>(
        &self,
        operation: &str,
        url: String,
        body: &B,
    ) -> Result<(), ProviderError> {
        let response = self.send(operation, self.http.put(url).json(body)).await?;

        Self::expect_success(operation, &response)
    }

    /// Reads a JSON body, refusing a non-success status first.
    async fn decode<T: DeserializeOwned>(
        operation: &str,
        response: reqwest::Response,
    ) -> Result<T, ProviderError> {
        if !response.status().is_success() {
            return Err(status_failure(operation, response.status()));
        }

        response
            .json()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }

    /// Discards a body, refusing a non-success status.
    fn expect_success(operation: &str, response: &reqwest::Response) -> Result<(), ProviderError> {
        if response.status().is_success() {
            Ok(())
        } else {
            Err(status_failure(operation, response.status()))
        }
    }
}
