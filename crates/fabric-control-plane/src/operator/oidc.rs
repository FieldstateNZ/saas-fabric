//! Establishing an operator from a token the platform's realm issued.
//!
//! # What this posture claims, and what it does not
//!
//! [`TrustedHeaderOperators`](super::TrustedHeaderOperators) trusts the
//! network: whoever the proxy says is calling, is. This trusts a signature
//! instead, which means the control plane no longer depends on being
//! unreachable to be safe. That is the point of it — the operator plane stays
//! a tailnet because defence in depth is worth having, not because the
//! application would be unauthenticated without it.
//!
//! Authority comes from a **realm role**, not from a list of names in a
//! configuration file. Adding an operator becomes something done in the
//! identity provider, where joiners and leavers are already handled, rather
//! than a deployment change.

mod bearer;
mod claims;
mod holder;
mod keys;
#[cfg(test)]
mod oidc_tests;
mod verification;

use std::sync::Arc;

use http::HeaderMap;
use jsonwebtoken::Algorithm;

use crate::logging;
use crate::operator::{Operator, OperatorAuthError, OperatorAuthenticator};

use bearer::bearer;

pub use holder::KeyHolder;
pub use keys::VerificationKeys;

/// Verifies an operator's bearer token against the platform's realm.
pub struct OidcOperators {
    /// The issuer every accepted token must name, matched exactly.
    issuer: String,

    /// The client an accepted token must have been issued to.
    client_id: String,

    /// The realm role that confers operator authority.
    required_role: String,

    /// The keys tokens are verified against, refreshed out of band.
    keys: Arc<KeyHolder>,

    /// How much clock skew to tolerate on `exp` and `nbf`, in seconds.
    leeway: u64,

    /// The one signature algorithm accepted.
    ///
    /// **Pinned at construction, never configurable.** An algorithm a
    /// deployment can choose is an algorithm an attacker can choose, and the
    /// classic JWT failure is a verifier that accepts whichever one the token
    /// names. Production is always RS256; the tests pin a symmetric algorithm
    /// so they can sign without a private key in the repository.
    algorithm: Algorithm,
}

impl OidcOperators {
    /// Builds an authenticator over a realm's issued tokens.
    ///
    /// # Errors
    ///
    /// Returns a message if the issuer, client or role is blank. Each would
    /// otherwise fail open in its own way — a blank issuer matches a token
    /// from anywhere, and a blank role is held by everyone.
    pub fn new(
        issuer: &str,
        client_id: &str,
        required_role: &str,
        keys: Arc<KeyHolder>,
        leeway: u64,
    ) -> Result<Self, String> {
        for (name, value) in [
            ("issuer", issuer),
            ("client_id", client_id),
            ("required_role", required_role),
        ] {
            if value.trim().is_empty() {
                return Err(format!("operator: {name} must not be empty"));
            }
        }

        Ok(Self {
            issuer: issuer.trim().trim_end_matches('/').to_owned(),
            client_id: client_id.trim().to_owned(),
            required_role: required_role.trim().to_owned(),
            keys,
            leeway,
            algorithm: Algorithm::RS256,
        })
    }

    /// The same authenticator, verifying HS256 so a test can sign a token.
    #[cfg(test)]
    pub(super) fn signed_symmetrically_for_tests(mut self) -> Self {
        self.algorithm = Algorithm::HS256;
        self
    }
}

impl OperatorAuthenticator for OidcOperators {
    fn authenticate(&self, headers: &HeaderMap) -> Result<Operator, OperatorAuthError> {
        let token = bearer(headers).ok_or(OperatorAuthError::Missing)?;
        let claims = self.verify(token)?;

        if claims.azp.as_deref() != Some(self.client_id.as_str()) || !claims.holds(&self.required_role) {
            logging::operator_refused("bearer token");
            return Err(OperatorAuthError::NotAnOperator);
        }

        // The bearer travels with the identity, because the platform acts on
        // the identity provider as this operator rather than as a service
        // account of its own (ADR 0012).
        Ok(Operator::new(
            claims.subject(),
            crate::operator::OperatorToken::new(token),
        ))
    }

    fn describe(&self) -> String {
        format!(
            "operator tokens from {} for client {}, requiring role {}",
            self.issuer, self.client_id, self.required_role
        )
    }
}
