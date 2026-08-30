//! Which HTTP status each outcome answers with.
//!
//! # The one that is easy to get wrong
//!
//! **A denial is not `403`.** `/v1/check` asks a question, and `allowed:
//! false` is a successful answer to it. A `403` would mean the caller may not
//! invoke the decision API at all — a different claim entirely, and one that
//! sends an operator looking in the wrong place.
//!
//! The rest of the table exists so that a transport concern cannot blur
//! distinctions the layers below took care to keep apart: a credential problem
//! is not an outage, an outage is not a misconfiguration, and none of the three
//! is a decision.

use axum::http::StatusCode;

use crate::{DecisionError, VerificationError};

/// The status a failed verification answers with.
///
/// Note what is absent: there is no branch that produces `403`, and none that
/// turns an unavailable identity provider into `401`. A caller whose token is
/// fine must never be told it is not because the provider is down.
pub(super) const fn for_verification(error: &VerificationError) -> StatusCode {
    match error {
        // The credential is not acceptable. Which check failed is logged and
        // never returned: naming it tells an attacker what to work on next.
        VerificationError::Refused(_) => StatusCode::UNAUTHORIZED,

        // Trust could not be established. The caller may be perfectly
        // entitled, and retrying later is the right advice.
        VerificationError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// The status a failed decision answers with.
pub(super) const fn for_decision(error: DecisionError) -> StatusCode {
    match error {
        DecisionError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,

        // The platform's own state or request was wrong. Emphatically not a
        // denial: the caller may hold the permission, and nothing they do will
        // make this work.
        DecisionError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
