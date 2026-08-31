//! Obtaining an anonymous pull token, and sending requests with it.

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::{Response, StatusCode};

use fabric_platform_management::RegistryError;

use crate::client::wire::PullToken;
use crate::client::OciRegistry;
use crate::errors::{status_failure, transport_failure};

impl OciRegistry {
    /// Sends a request with a pull token, minting one if needed.
    ///
    /// Retries once on `401` with a fresh token. A cached token has no expiry
    /// recorded against it, so ageing out is noticed here rather than
    /// predicted — which is the same path a token revoked early would take, so
    /// there is one mechanism instead of two.
    pub(super) async fn get(
        &self,
        operation: &str,
        repository: &str,
        url: &str,
        accept: &str,
    ) -> Result<Response, RegistryError> {
        let token = self.token(operation, repository, false).await?;
        let response = self.attempt(operation, url, accept, &token).await?;

        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        let token = self.token(operation, repository, true).await?;

        self.attempt(operation, url, accept, &token).await
    }

    /// Sends one request.
    async fn attempt(
        &self,
        operation: &str,
        url: &str,
        accept: &str,
        token: &str,
    ) -> Result<Response, RegistryError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_str(accept).map_err(|_| RegistryError::Refused {
                detail: format!("{operation}: the Accept header is not sendable"),
            })?,
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("saas-fabric-control-plane"));

        self.http
            .get(url)
            .headers(headers)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }

    /// A pull token for one repository, from the cache unless `fresh`.
    async fn token(&self, operation: &str, repository: &str, fresh: bool) -> Result<String, RegistryError> {
        let path = self.path(repository).to_owned();

        if !fresh {
            if let Some(cached) = self.cached(&path) {
                return Ok(cached);
            }
        }

        let url = format!(
            "{base}/token?service={host}&scope=repository:{path}:pull",
            base = self.base_url,
            host = self.registry_host
        );

        let response = self
            .http
            .get(&url)
            .header(USER_AGENT, "saas-fabric-control-plane")
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))?;

        if !response.status().is_success() {
            return Err(status_failure(operation, response.status(), response.headers()));
        }

        let minted: PullToken = response
            .json()
            .await
            .map_err(|error| transport_failure(operation, &error))?;

        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(path, minted.token.clone());
        }

        Ok(minted.token)
    }

    /// The cached token for a repository path, if there is one.
    fn cached(&self, path: &str) -> Option<String> {
        self.tokens.lock().ok()?.get(path).cloned()
    }
}
