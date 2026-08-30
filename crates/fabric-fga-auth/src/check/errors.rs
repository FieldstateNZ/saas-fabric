//! Why a decision could not be reached.
//!
//! Never a denial. A caller told "denied" when the platform is broken goes and
//! asks an administrator for access they already have, and the incident stays
//! hidden behind a message about permissions.

/// Why the operation could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecisionError {
    /// The authorization service could not be reached or failed. Answers `503`.
    #[error("the authorization service is unavailable")]
    Unavailable,

    /// The platform's own state or request was wrong. Answers `500`.
    ///
    /// An unknown store or model, a request the service rejected as malformed,
    /// or a response that could not be read. All of them mean *this platform*
    /// is misconfigured — the caller may well hold the permission, and nothing
    /// they do will fix it.
    #[error("the authorization request could not be made")]
    Internal,
}

/// Why the authorization service could not answer.
///
/// The same split as [`DecisionError`], named separately so an adapter states
/// which kind of failure it saw rather than returning a string for the
/// operation to guess from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecisionFailure {
    /// Refused a connection, timed out, or answered `5xx`.
    #[error("unreachable")]
    Unavailable,

    /// Answered `4xx`, named a store or model it does not have, or returned
    /// something that is not a decision.
    #[error("the request or the platform's state is wrong")]
    Internal,
}
