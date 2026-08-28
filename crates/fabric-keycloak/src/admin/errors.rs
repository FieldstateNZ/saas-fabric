//! Turning transport and status failures into provider errors.

use fabric_reconciliation::ProviderError;
use reqwest::StatusCode;

/// Builds the right error for a failure from `.send()`.
///
/// Every transport failure is [`Unavailable`](ProviderError::Unavailable),
/// including a timeout that may have fired after Keycloak had already acted.
/// That is a deliberate difference from the runtime plane's connector client,
/// which draws a careful distinction between "certainly not applied" and
/// "outcome unknown" — because there, the operation may be a write whose
/// duplicate would corrupt a tenant's data.
///
/// Here it does not matter: every operation this adapter performs is
/// idempotent by the port's contract, so re-attempting one whose outcome is
/// unknown is safe by construction. Modelling a distinction nothing acts on
/// would be ceremony.
///
/// The message is the adapter's own classification, never
/// `reqwest::Error`'s `Display` — that can carry the full URL, and this text
/// reaches an operator's screen.
pub(super) fn transport_failure(operation: &str, error: &reqwest::Error) -> ProviderError {
    let kind = if error.is_connect() {
        "could not connect"
    } else if error.is_timeout() {
        "timed out"
    } else if error.is_decode() {
        "returned a response that could not be read"
    } else {
        "failed"
    };

    ProviderError::Unavailable {
        detail: format!("{operation} {kind}"),
    }
}

/// Builds the right error for a status Keycloak returned.
///
/// # The three groups, and why they are not one
///
/// - **401 and 403** mean the platform's own machine credential is wrong or
///   under-privileged. Retrying will not fix it, and it is not Keycloak being
///   unwell, so it is its own variant.
/// - **Other 4xx** mean Keycloak understood and refused. That is a statement
///   about the desired state, which an operator has to act on.
/// - **5xx** mean Keycloak is unwell, which the next sweep may find resolved.
///
/// The response body never appears. Keycloak's admin errors quote realm
/// internals and occasionally echo request content, and this text is shown to
/// an operator and written to a log (§23).
pub(super) fn status_failure(operation: &str, status: StatusCode) -> ProviderError {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return ProviderError::NotPermitted;
    }

    if status.is_server_error() {
        return ProviderError::Unavailable {
            detail: format!("{operation} returned {}", status.as_u16()),
        };
    }

    ProviderError::Rejected {
        detail: format!("{operation} was refused with {}", status.as_u16()),
    }
}
