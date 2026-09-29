//! Sending a request with a pull token, through one of the two clients.

use reqwest::header::ACCEPT;
use reqwest::{Method, Response, StatusCode};

use fabric_platform_management::RegistryError;

use crate::client::OciRegistry;
use crate::errors::send_failure;

/// Which of the two clients a request goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Via {
    /// Same-origin redirects only.
    Api,

    /// Redirects to any HTTPS origin, for a blob's CDN.
    Blob,
}

impl OciRegistry {
    /// `GET` through the API client.
    pub(super) async fn get(
        &self,
        operation: &str,
        repository: &str,
        url: &str,
        accept: &str,
    ) -> Result<Response, RegistryError> {
        self.send(Method::GET, Via::Api, operation, repository, url, accept)
            .await
    }

    /// Sends a request with a pull token, minting one if needed.
    ///
    /// Retries once on `401` with a fresh token. A cached token has no expiry
    /// recorded against it, so ageing out is noticed here rather than
    /// predicted — which is the same path a token revoked early would take, so
    /// there is one mechanism instead of two. A second `401` is the caller's
    /// to classify: a refusal after a token was issued, never an answer.
    pub(super) async fn send(
        &self,
        method: Method,
        via: Via,
        operation: &str,
        repository: &str,
        url: &str,
        accept: &str,
    ) -> Result<Response, RegistryError> {
        let token = self.token(operation, repository, false).await?;
        let response = self.attempt(&method, via, operation, url, accept, &token).await?;

        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        let token = self.token(operation, repository, true).await?;

        self.attempt(&method, via, operation, url, accept, &token).await
    }

    /// Sends one request.
    async fn attempt(
        &self,
        method: &Method,
        via: Via,
        operation: &str,
        url: &str,
        accept: &str,
        token: &str,
    ) -> Result<Response, RegistryError> {
        let client = match via {
            Via::Api => &self.api,
            Via::Blob => &self.blobs,
        };

        client
            .request(method.clone(), url)
            .header(ACCEPT, accept)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| send_failure(operation, &error))
    }
}
