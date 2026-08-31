//! Applying the headers every request needs, and reading the answer.

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;

use crate::host::failures::{status_failure, transport_failure};
use crate::host::{PlatformGitRepository, API_VERSION, API_VERSION_HEADER};
use crate::PlatformGitError;

impl PlatformGitRepository {
    /// Sends a request and decodes a successful JSON body.
    ///
    /// `what` names the thing being addressed, so a `404` can say which path
    /// or ref was missing rather than "something".
    pub(crate) async fn json<T: DeserializeOwned>(
        &self,
        operation: &str,
        method: Method,
        url: String,
        body: Option<serde_json::Value>,
        what: Option<&str>,
    ) -> Result<T, PlatformGitError> {
        let response = self.send(operation, method, url, body).await?;
        let status = response.status();

        if !status.is_success() {
            return Err(status_failure(operation, status, response.headers(), what));
        }

        response
            .json()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }

    /// Sends a request, retrying once on `401` with a freshly minted bearer.
    ///
    /// An installation token can stop working inside its stated lifetime — the
    /// App uninstalled, its key rotated, its installation suspended. Without
    /// this, every call would present the dead token until its local deadline
    /// passed.
    pub(crate) async fn send(
        &self,
        operation: &str,
        method: Method,
        url: String,
        body: Option<serde_json::Value>,
    ) -> Result<Response, PlatformGitError> {
        let response = self
            .attempt(operation, method.clone(), url.clone(), body.clone())
            .await?;

        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        self.bearers.invalidate().await;

        self.attempt(operation, method, url, body).await
    }

    /// Applies the headers, a current bearer, and sends once.
    async fn attempt(
        &self,
        operation: &str,
        method: Method,
        url: String,
        body: Option<serde_json::Value>,
    ) -> Result<Response, PlatformGitError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/vnd.github+json"));
        headers.insert(API_VERSION_HEADER, HeaderValue::from_static(API_VERSION));
        // Required by the host, which refuses requests without one.
        headers.insert(USER_AGENT, HeaderValue::from_static("saas-fabric-control-plane"));

        let bearer = self.bearers.bearer(&self.http).await?;

        let mut builder = self
            .http
            .request(method, url)
            .headers(headers)
            .bearer_auth(bearer);

        if let Some(body) = body {
            builder = builder.json(&body);
        }

        builder
            .send()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }
}
