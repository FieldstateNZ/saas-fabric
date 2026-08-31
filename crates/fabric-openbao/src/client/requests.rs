//! Sending a request, and sending one inside a client's namespace.
//!
//! Kept apart from the client's construction so that the retry rule and the
//! namespace rule sit together, which is where somebody looking for either of
//! them will look.

use super::OpenBao;

impl OpenBao {
    /// Sends a request inside a client's namespace.
    ///
    /// The namespace is a **header**, not a path segment, which is what makes
    /// it a boundary the store enforces rather than a prefix this code
    /// assembles. Measured: the same path read without the header, and read in
    /// another namespace, both answer `404`.
    pub(crate) async fn send_in(
        &self,
        namespace: &str,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        let first = self
            .attempt_in(Some(namespace), method.clone(), url, body.clone())
            .await?;

        if first.status() != reqwest::StatusCode::FORBIDDEN {
            return Ok(first);
        }

        self.tokens.invalidate().await;
        self.attempt_in(Some(namespace), method, url, body).await
    }

    /// Sends a request, logging in again once if the token was refused.
    ///
    /// The retry is not politeness. A store token can be revoked before its
    /// lease expires, and the platform learns that as a `403` on an ordinary
    /// call — the same shape of failure the Keycloak adapter handles the same
    /// way, and for the same reason.
    pub(crate) async fn send(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        let first = self.attempt(method.clone(), url, body.clone()).await?;

        if first.status() != reqwest::StatusCode::FORBIDDEN {
            return Ok(first);
        }

        self.tokens.invalidate().await;
        self.attempt(method, url, body).await
    }

    /// One attempt, carrying whatever token is current.
    async fn attempt(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        self.attempt_in(None, method, url, body).await
    }

    /// One attempt, optionally inside a client's namespace.
    async fn attempt_in(
        &self,
        namespace: Option<&str>,
        method: reqwest::Method,
        url: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, String> {
        let token = self.tokens.token(&self.http).await?;

        let mut request = self.http.request(method, url).header("X-Vault-Token", token);

        if let Some(namespace) = namespace {
            request = request.header("X-Vault-Namespace", namespace);
        }

        if let Some(body) = body {
            request = request.json(&body);
        }

        request
            .send()
            .await
            .map_err(|_| "the secret store could not be reached".to_owned())
    }
}
