//! Proving to OpenBao that this pod is who it says it is.
//!
//! # Kubernetes auth, not a token in a file
//!
//! A static token would be exactly the credential this whole change exists to
//! stop transporting: somebody would have to create it, put it somewhere, and
//! rotate it. The pod already holds an identity its orchestrator issued, so it
//! presents that and OpenBao decides what it may reach.

mod login;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_core::Clock;
use tokio::sync::Mutex;

/// How long before a lease expires the platform stops using its token.
///
/// Sixty seconds, matching the Keycloak adapter: enough that a request started
/// just under the wire still completes.
pub(super) const EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// The shortest a token is treated as usable for.
///
/// A store handing out very short leases should still make progress rather
/// than sending this into a login on every call.
pub(super) const MINIMUM_LIFETIME: Duration = Duration::from_secs(30);

/// A token, and when to stop presenting it.
pub(super) struct Held {
    /// The token itself. Never logged.
    pub(super) value: String,

    /// When it should no longer be presented.
    pub(super) good_until: Instant,
}

/// Logs in when needed and holds the resulting token.
pub(crate) struct TokenCache {
    /// Where the login endpoint is.
    pub(super) endpoint: String,

    /// The role to log in as.
    pub(super) role: String,

    /// Where the pod's own token is mounted.
    pub(super) token_path: String,

    /// The current token.
    held: Mutex<Option<Held>>,

    /// Measures token lifetime.
    pub(super) clock: Arc<dyn Clock>,
}

impl TokenCache {
    /// Builds a cache over one login endpoint.
    pub(crate) fn new(
        address: &str,
        auth_mount: &str,
        role: &str,
        token_path: &str,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            endpoint: format!("{}/v1/auth/{auth_mount}/login", address.trim_end_matches('/')),
            role: role.to_owned(),
            token_path: token_path.to_owned(),
            held: Mutex::new(None),
            clock,
        }
    }

    /// The token to present, logging in if there is not a usable one.
    pub(crate) async fn token(&self, http: &reqwest::Client) -> Result<String, String> {
        let mut held = self.held.lock().await;

        if let Some(current) = held.as_ref() {
            if self.clock.now() < current.good_until {
                return Ok(current.value.clone());
            }
        }

        let fresh = self.login(http).await?;
        let value = fresh.value.clone();
        *held = Some(fresh);

        Ok(value)
    }

    /// Drops the held token, so the next call logs in again.
    ///
    /// Called after a refusal: a token can be revoked before its lease is up,
    /// and the platform finds that out as a `403` rather than as an expiry.
    pub(crate) async fn invalidate(&self) {
        *self.held.lock().await = None;
    }
}
