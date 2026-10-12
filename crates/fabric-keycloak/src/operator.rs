//! Reading the signing keys of the realm platform operators sign in to.
//!
//! # Why this lives here and not in the control plane
//!
//! Where a realm publishes its signing keys is Keycloak's protocol. The control
//! plane verifies operator bearers against those keys and never learns where
//! they came from, which is the same boundary ADR 0008 draws for realms and
//! roles.
//!
//! # Why this is all that is left of operator sign-in here
//!
//! Sign-in belongs to the gateway (ADR 0024 §2): it runs the authorization-code
//! flow and forwards the access token as a bearer. The control plane no longer
//! redeems a code, so this adapter has no token endpoint and no authorization
//! endpoint to derive — only the key set every forwarded bearer is checked
//! against.
//!
//! The realm this reads is the platform's own, not a client's. No tenant
//! identity can be minted here and none is read.

#[cfg(test)]
mod operator_tests;
mod signing_keys;

/// Reads one realm's signing keys from the address this process reaches it at.
///
/// Holds one derived endpoint and an HTTP client. No credential: a realm's key
/// set is public.
pub struct OperatorRealmKeys {
    /// Where the realm publishes the keys it signs tokens with.
    jwks_endpoint: String,

    /// The HTTP client.
    http: reqwest::Client,
}

impl OperatorRealmKeys {
    /// Builds a key reader for one realm.
    ///
    /// `reachable_at` is where **this process** reaches the realm, which on a
    /// cluster is usually a service address rather than the public issuer a
    /// token names. Using the issuer there is the mistake that fails quietly:
    /// the pod cannot resolve it, no key set arrives, and every operator is
    /// refused with a log that says only that.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new(reachable_at: &str, timeout: std::time::Duration) -> Result<Self, String> {
        let reachable_at = reachable_at.trim().trim_end_matches('/');

        Ok(Self {
            jwks_endpoint: format!("{reachable_at}/protocol/openid-connect/certs"),
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|error| format!("operator realm keys: {error}"))?,
        })
    }
}
