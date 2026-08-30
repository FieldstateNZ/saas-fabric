//! The three ways verification fails, which are not the same failure.
//!
//! Collapsing them is the mistake this module exists to prevent. A provider
//! outage that answers `401` tells a legitimate operator their credentials are
//! wrong, sends them to re-authenticate against the very thing that is down,
//! and hides an incident behind a message about the user. The status is the
//! only signal most of the way out, so it has to carry the distinction.

/// A registry that cannot be trusted, found before anything is served.
///
/// Every variant is fatal at startup rather than per request. A verifier with
/// no issuers refuses everything; a verifier with a duplicated issuer refuses
/// or accepts depending on map ordering. Neither is a state to discover from a
/// request, so neither is reachable from one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigurationError {
    /// The registry names no issuers.
    ///
    /// Refused rather than defaulted. An empty registry is the shape in which
    /// an authorization service quietly trusts nothing — or, in at least one
    /// real implementation, quietly trusts *everything*.
    #[error("the issuer registry is empty; a verifier that trusts no issuer cannot authenticate anybody")]
    NoIssuers,

    /// Two registrations claim the same issuer.
    #[error(
        "issuer {issuer} is registered more than once; which registration wins would depend on map ordering"
    )]
    DuplicateIssuer {
        /// The issuer named twice.
        issuer: String,
    },

    /// A registration is not usable as written.
    #[error("issuer {issuer}: {detail}")]
    InvalidRegistration {
        /// The issuer the registration named, for locating it.
        issuer: String,
        /// What was wrong with it.
        detail: String,
    },
}

/// A presented token that cannot be turned into a verified identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerificationError {
    /// The caller's credential is not acceptable. Answers `401`.
    ///
    /// One variant for every cause on purpose. The *reason* is worth logging
    /// and is never worth returning: telling a caller which check failed tells
    /// an attacker which one to work on next.
    #[error("the presented token was refused")]
    Refused(
        /// Why, for the log and never for the response.
        RefusalReason,
    ),

    /// Trust could not be established, and the credential may be perfectly
    /// good. Answers `503`.
    #[error("verification is temporarily unavailable: {0}")]
    Unavailable(
        /// Which part of establishing trust failed.
        UnavailableReason,
    ),
}

/// Why a credential was refused. Logged, never returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RefusalReason {
    /// The token is not a JWT this verifier can read at all.
    #[error("malformed token")]
    Malformed,

    /// No claim naming an issuer.
    #[error("no issuer claim")]
    NoIssuer,

    /// An issuer that is not in the registry.
    #[error("unknown issuer")]
    UnknownIssuer,

    /// An algorithm outside the ones this issuer's registration permits.
    ///
    /// Distinct from a bad signature: the token may be perfectly signed with
    /// an algorithm nobody agreed to accept.
    #[error("algorithm not permitted for this issuer")]
    DisallowedAlgorithm,

    /// The signature did not verify against a key that is genuinely current.
    #[error("signature did not verify")]
    BadSignature,

    /// A `kid` that is absent from a key set known to be fresh.
    #[error("signing key not published by this issuer")]
    UnknownKey,

    /// Outside the token's validity window.
    #[error("token expired or not yet valid")]
    OutsideValidity,

    /// The audience does not include what this issuer's registration requires.
    #[error("audience does not match")]
    WrongAudience,

    /// No subject to name.
    #[error("no subject claim")]
    NoSubject,

    /// A subject that cannot be part of a principal — see `fabric_core::SubjectId`.
    #[error("subject is not a usable identifier")]
    UnusableSubject,
}

/// Why trust could not be established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnavailableReason {
    /// The key set could not be fetched and nothing cached could serve.
    #[error("the issuer's keys could not be fetched")]
    KeysUnreachable,

    /// The cached key set is older than the registration permits.
    ///
    /// The case where continuing to serve is the *wrong* instinct: a key
    /// removed during an outage would otherwise stay trusted indefinitely.
    #[error("the cached key set is too old to be trusted")]
    KeysTooOld,
}
