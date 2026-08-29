//! Signing platform operators in against a realm.
//!
//! # Why this lives here and not in the control plane
//!
//! Everything below is Keycloak's protocol: where its authorization endpoint
//! is, where its token endpoint is, where it publishes its signing keys, and
//! what a redemption form looks like. The control plane holds the *port* —
//! [`OperatorSignIn`] — and never learns any of it, which is the same
//! boundary ADR 0008 draws for realms and roles.
//!
//! The realm this authenticates against is the platform's own, not a client's.
//! No tenant identity can be minted here and none is read.
//!
//! A realm has **two** addresses on a cluster — the issuer a browser uses and
//! the service address this pod uses. The `endpoints` module is where that is
//! reasoned about.

mod endpoints;
#[cfg(test)]
mod operator_tests;
mod signing_keys;

use async_trait::async_trait;
use fabric_control_plane::{IssuedToken, OperatorSignIn, SignInError};

use crate::wire::TokenResponse;

/// Signs operators in against one realm.
///
/// Holds three derived endpoints and an HTTP client. No credential: the
/// console is a public client and PKCE replaces one.
pub struct RealmSignIn {
    /// Where the browser authenticates.
    authorization_endpoint: String,

    /// Where an authorization code is redeemed.
    token_endpoint: String,

    /// Where the realm publishes the keys it signs tokens with.
    jwks_endpoint: String,

    /// The client the console authenticates as.
    client_id: String,

    /// Where the provider returns the browser, sent again at redemption
    /// because the provider requires it to match the authorization request.
    redirect_uri: String,

    /// The HTTP client.
    http: reqwest::Client,
}

impl RealmSignIn {
    /// Builds a sign-in against one realm.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    /// Builds a sign-in against one realm.
    ///
    /// `issuer` is what appears in a token and where a **browser** is sent.
    /// `reachable_at` is where **this process** reaches the same realm. Those
    /// are two questions on a cluster, and the module that derives the
    /// endpoints explains why.
    ///
    /// # Errors
    ///
    /// Returns a message if the HTTP client cannot be built.
    pub fn new(
        issuer: &str,
        reachable_at: &str,
        client_id: &str,
        redirect_uri: &str,
        timeout: std::time::Duration,
    ) -> Result<Self, String> {
        let endpoints = endpoints::derive(issuer, reachable_at);

        Ok(Self {
            authorization_endpoint: endpoints.authorization,
            token_endpoint: endpoints.token,
            jwks_endpoint: endpoints.jwks,
            client_id: client_id.to_owned(),
            redirect_uri: redirect_uri.to_owned(),
            http: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|error| format!("operator sign-in: {error}"))?,
        })
    }
}

#[async_trait]
impl OperatorSignIn for RealmSignIn {
    fn authorization_endpoint(&self) -> &str {
        &self.authorization_endpoint
    }

    async fn redeem(&self, code: &str, verifier: &str) -> Result<IssuedToken, SignInError> {
        // A public client: no secret in this form, and none held anywhere.
        // The verifier is what proves this redemption belongs to the browser
        // that asked for the code.
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.redirect_uri.as_str()),
            ("client_id", self.client_id.as_str()),
            ("code_verifier", verifier),
        ];

        let response = self
            .http
            .post(&self.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|_| SignInError::Unavailable)?;

        // A 4xx is the provider telling us the code is spent, expired, or was
        // not issued for this verifier. It is not retryable and its body is
        // not repeated to the browser.
        if !response.status().is_success() {
            return Err(if response.status().is_client_error() {
                SignInError::Refused
            } else {
                SignInError::Unavailable
            });
        }

        let token: TokenResponse = response.json().await.map_err(|_| SignInError::Unavailable)?;

        Ok(IssuedToken {
            access_token: token.access_token,
            expires_in: token.expires_in,
        })
    }
}
