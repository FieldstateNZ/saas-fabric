//! The Fabric authorization front's identity primitive.
//!
//! ```text
//! Bearer <the tenant user's own token>
//!       ↓
//! unverified `iss`            read to select a registration, trusted for nothing
//!       ↓
//! trusted issuer registry     this crate
//!       ↓
//! issuer-specific keys        fetched from an address Fabric configured
//!       ↓
//! verify signature, iss, aud, exp, nbf, alg
//!       ↓
//! VerifiedIdentity { tenant, subject, principal, store }
//! ```
//!
//! # What this crate is for
//!
//! One question: *given a token, who is this and which store answers for
//! them?* It makes no authorization decision, holds no policy, and — in this
//! increment — talks to no authorization service at all. Everything that comes
//! later is allowed to trust its answer, which is why it is worth building
//! alone and testing adversarially before anything depends on it.
//!
//! # The rule that shapes every type here
//!
//! **The caller supplies a token and nothing else.** Which tenant they belong
//! to, which realm qualifies their principal, and which store answers for them
//! are properties of the registration selected by the *verified* issuer. A
//! token carrying a `tenant`, `store_id` or `principal` claim is never wrong
//! to carry it; this crate simply never reads it.
//!
//! # Three failures, three answers
//!
//! A bad credential and an unreachable provider are not the same event and
//! must not answer the same way (ADR 0016). See [`ConfigurationError`] —
//! fatal at startup — and [`VerificationError`], which separates a refused
//! credential from trust that could not be established.

mod cache;
mod errors;
mod identity;
mod keys;
mod registry;
mod verifier;

pub use cache::KeyCache;
pub use errors::{ConfigurationError, RefusalReason, UnavailableReason, VerificationError};
pub use identity::VerifiedIdentity;
pub use keys::{KeySet, KeySource};
pub use registry::{IssuerRegistration, Registry};
pub use verifier::Verifier;
