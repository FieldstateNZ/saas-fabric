//! Reading an issuer's signing keys over HTTP.
//!
//! # Why the address is per issuer, unlike the authorization service's
//!
//! `OpenFgaDecisions` is built from a port and cannot be pointed anywhere,
//! because it talks to a process inside the same container. This is the
//! opposite case by design: a `jwks_uri` is genuinely different per issuer and
//! is usually an address only this process can reach.
//!
//! It is still not caller-controlled. The URL comes from the registration the
//! **verified** issuer selected, never from a claim in an incoming token — the
//! distinction that lets Fabric read keys from a cluster-local Keycloak while
//! tokens carry a public issuer, without the request-forgery exposure that
//! makes other implementations refuse private addresses outright (ADR 0016).

use async_trait::async_trait;

use crate::{KeySet, KeySource};

/// How long to wait for a key set.
const TIMEOUT_SECONDS: u64 = 10;

/// Reads key sets from the address a registration names.
pub struct HttpKeySource {
    /// The HTTP client.
    http: reqwest::Client,
}

impl HttpKeySource {
    /// Builds a key source.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(TIMEOUT_SECONDS))
                .build()
                .map_err(|error| format!("key source: {error}"))?,
        })
    }
}

#[async_trait]
impl KeySource for HttpKeySource {
    async fn fetch(&self, jwks_uri: &str) -> Result<KeySet, String> {
        let response = self
            .http
            .get(jwks_uri)
            .send()
            .await
            .map_err(|error| format!("keys unreachable: {error}"))?;

        if !response.status().is_success() {
            return Err(format!("keys unavailable: {}", response.status()));
        }

        let document = response
            .text()
            .await
            .map_err(|error| format!("keys unreadable: {error}"))?;

        KeySet::from_jwks(&document)
    }
}
