//! Failures that prevent a tenant identity context being established.

use axum::response::{IntoResponse, Response};
use http::StatusCode;

/// Why a request could not be given a tenant identity context.
///
/// Every variant is a **rejection**. The specification requires the platform to
/// fail closed when tenant context cannot be safely resolved (§28), so there is
/// deliberately no variant meaning "carry on without a tenant" and no default
/// tenant to fall back to.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    /// No `Authorization` header was present.
    #[error("request has no Authorization header")]
    MissingAuthorization,

    /// The `Authorization` header was present but was not a bearer token.
    #[error("Authorization header is not a Bearer token")]
    NotBearer,

    /// The token was structurally invalid — not three dot-separated segments,
    /// or a payload that was not base64url-encoded JSON.
    #[error("bearer token is malformed")]
    MalformedToken,

    /// The token's signature did not verify, or a registered claim such as
    /// `exp`, `iss`, or `aud` failed validation.
    ///
    /// The underlying reason is deliberately not exposed to the caller; it is
    /// logged instead. Telling an attacker precisely which check failed is free
    /// help.
    #[error("bearer token failed verification")]
    UnverifiedToken,

    /// The token has expired.
    #[error("bearer token has expired")]
    ExpiredToken,

    /// The token's `nbf` (not-before) is still in the future.
    ///
    /// Distinct from [`Self::ExpiredToken`] so that operators can tell the two
    /// ends of the validity window apart in logs — a burst of these usually
    /// means a clock has drifted, not that anybody is attacking. The caller is
    /// told no more than "not yet valid" either way.
    #[error("bearer token is not yet valid")]
    TokenNotYetValid,

    /// The token carried no tenant claim under the configured name.
    ///
    /// Specification §28: missing tenant claim, request rejected.
    #[error("bearer token has no {claim} claim")]
    MissingTenantClaim {
        /// The configured claim name that was looked for.
        claim: String,
    },

    /// The tenant claim was present but was not a valid tenant identifier.
    ///
    /// The offending value is not echoed back — it is attacker-controlled.
    #[error("the {claim} claim is not a valid tenant identifier")]
    InvalidTenantClaim {
        /// The configured claim name that held the bad value.
        claim: String,
    },

    /// The request carried a tenant-selection header.
    ///
    /// Specification §11 forbids selecting a tenant through a caller-provided
    /// header. The request is rejected rather than the header being ignored, so
    /// that a caller who believes the header works learns otherwise at once.
    #[error("tenant selection through the {header} header is not permitted")]
    TenantHeaderPresent {
        /// The banned header that was found on the request.
        header: &'static str,
    },
}

impl IdentityError {
    /// Maps the failure to the status code the caller sees.
    ///
    /// Everything that concerns the *token* is a 401, because presenting a
    /// different token could succeed. The banned tenant header is a 400,
    /// because the request itself is malformed and re-authenticating would not
    /// help.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::TenantHeaderPresent { .. } => StatusCode::BAD_REQUEST,
            _ => StatusCode::UNAUTHORIZED,
        }
    }
}

impl IntoResponse for IdentityError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Safe to return: every message here describes the shape of the
        // request, never the contents of the token or the tenant's existence.
        (status, self.to_string()).into_response()
    }
}
