//! Speaking to the OpenFGA embedded beside this process.
//!
//! # The destination is not configuration
//!
//! This adapter is built against **loopback and a port**, never a URL. There
//! is no per-request destination and none in the issuer registry, because
//! having made the store and the model trustworthy it would be careless to
//! leave the last hop pointable at anything. The service it talks to is inside
//! the same container, listening on `127.0.0.1` with no authentication of its
//! own (ADR 0016) — an address that could be redirected is the one thing that
//! would make that arrangement dangerous rather than contained.

use async_trait::async_trait;
use serde::Deserialize;

use crate::{DecisionFailure, Decisions};

/// How long to wait for a decision.
///
/// The service is in the same container, so this is generous for a local call
/// and still short enough that a wedged process fails a request rather than
/// holding it open.
const TIMEOUT_SECONDS: u64 = 5;

/// The OpenFGA running beside this process.
pub struct OpenFgaDecisions {
    /// `http://127.0.0.1:<port>`, built here and nowhere else.
    base: String,

    /// The HTTP client.
    http: reqwest::Client,
}

impl OpenFgaDecisions {
    /// Builds an adapter against the loopback service on `port`.
    ///
    /// Takes a port rather than an address on purpose: there is no argument
    /// that can point this at another host.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn on_loopback(port: u16) -> Result<Self, String> {
        Ok(Self {
            base: format!("http://127.0.0.1:{port}"),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(TIMEOUT_SECONDS))
                .build()
                .map_err(|error| format!("authorization client: {error}"))?,
        })
    }
}

#[async_trait]
impl Decisions for OpenFgaDecisions {
    async fn check(
        &self,
        store: &str,
        model: &str,
        user: &str,
        relation: &str,
        object: &str,
    ) -> Result<bool, DecisionFailure> {
        let body = serde_json::json!({
            "authorization_model_id": model,
            "tuple_key": { "user": user, "relation": relation, "object": object },
        });

        let response = self
            .http
            .post(format!("{}/stores/{store}/check", self.base))
            .json(&body)
            .send()
            .await
            // Refused, reset, or timed out: the service is not answering.
            .map_err(|_| DecisionFailure::Unavailable)?;

        let status = response.status();

        if status.is_server_error() {
            return Err(DecisionFailure::Unavailable);
        }

        // A 4xx here is never the caller's fault: they supplied a relation and
        // an object, and everything else in the request came from this
        // platform. An unknown store or model, or a body the service rejects,
        // means *we* are wrong — and answering "denied" would hide it behind a
        // permissions message while the caller goes asking for access they
        // already have.
        if !status.is_success() {
            return Err(DecisionFailure::Internal);
        }

        response
            .json::<CheckResponse>()
            .await
            .map(|decision| decision.allowed)
            .map_err(|_| DecisionFailure::Internal)
    }

    async fn reachable(&self) -> bool {
        self.healthy().await
    }
}

impl OpenFgaDecisions {
    /// Asks the service's own health endpoint.
    async fn healthy(&self) -> bool {
        self.http
            .get(format!("{}/healthz", self.base))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
    }
}

/// The only part of the answer this crate reads.
#[derive(Deserialize)]
struct CheckResponse {
    /// Whether the relation holds.
    allowed: bool,
}
