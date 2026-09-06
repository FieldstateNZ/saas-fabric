//! Failures that prevent a tenant identity context being established.
//!
//! Over the 120-line guidance deliberately: this is one enum, one status
//! mapping, and one `IntoResponse`, and every variant exists only because the
//! enum does. Splitting the refusals across files would put the exhaustive
//! `status` match somewhere other than the variants it must cover, which is the
//! one thing keeping a new refusal from silently defaulting to the wrong code.

use axum::response::{IntoResponse, Response};
use http::StatusCode;

/// Why a request could not be given a tenant identity context.
///
/// Every variant is a **rejection**. The specification requires failing closed
/// when tenant context cannot be safely resolved (§28), so there is
/// deliberately no variant meaning "carry on without a tenant", and no default.
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
    /// The underlying reason is not exposed to the caller — logged instead,
    /// since telling an attacker precisely which check failed is free help.
    #[error("bearer token failed verification")]
    UnverifiedToken,

    /// The token has expired.
    #[error("bearer token has expired")]
    ExpiredToken,

    /// The token's `nbf` (not-before) is still in the future.
    ///
    /// Distinct from [`Self::ExpiredToken`] so operators can tell the validity
    /// window's two ends apart in logs — a burst usually means clock drift,
    /// not an attack. Either way the caller is told no more than "not yet valid".
    #[error("bearer token is not yet valid")]
    TokenNotYetValid,

    /// The token carried no `iss` claim.
    ///
    /// Refused rather than treated as unregistered-but-harmless: ADR 0002
    /// records the same hole in the defence-in-depth allowlists, where simply
    /// omitting `iss` sailed past an issuer allowlist that then did nothing.
    #[error("bearer token has no iss claim")]
    MissingIssuerClaim,

    /// The token's issuer is in no registration, so it names no tenant.
    ///
    /// Never echoed back — that would make this message a confirmation
    /// oracle. It **is** logged, sanitised and bounded (see the `logging`
    /// module): a burst of these is an operator's only signal that the edge
    /// admits an issuer this registry does not know.
    #[error("the bearer token's issuer is not registered with this runtime")]
    UnregisteredIssuer,

    /// The token carried no tenant claim under the configured name.
    ///
    /// Specification §28. Still required after ADR 0019 §2: a token minted
    /// without the canonical claim comes from a realm not configured the way
    /// §10 describes, and admitting it would mean silently accepting two
    /// token shapes where the specification names one.
    #[error("bearer token has no {claim} claim")]
    MissingTenantClaim {
        /// The configured claim name that was looked for.
        claim: String,
    },

    /// The tenant claim named a different tenant than the issuer's registration.
    ///
    /// Neither value is echoed. This is a request to pick, not to
    /// disambiguate — and the only signal this process, which verifies
    /// nothing itself, will ever get that the edge and the registry diverged.
    #[error("the {claim} claim does not name the tenant its issuer is registered to")]
    TenantClaimDisagreesWithIssuer {
        /// The configured claim name that held the disagreeing value.
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
    /// Everything concerning the *token* is a 401 — presenting a different one
    /// could succeed — including the three issuer-binding refusals, ADR 0019's
    /// credential class: an unregistered issuer is a credential this
    /// deployment will not accept, not a fault of its own configuration. The
    /// banned tenant header is a 400: the request itself is malformed, and
    /// re-authenticating would not help.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::MissingAuthorization
            | Self::NotBearer
            | Self::MalformedToken
            | Self::UnverifiedToken
            | Self::ExpiredToken
            | Self::TokenNotYetValid
            | Self::MissingIssuerClaim
            | Self::UnregisteredIssuer
            | Self::MissingTenantClaim { .. }
            | Self::TenantClaimDisagreesWithIssuer { .. }
            | Self::InvalidTenantClaim { .. } => StatusCode::UNAUTHORIZED,
            Self::TenantHeaderPresent { .. } => StatusCode::BAD_REQUEST,
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
