//! The two windows that govern trust material, which are not one window.
//!
//! Conflating them is the mistake this module exists to prevent: the timer
//! that protects the issuer must never decide an authentication result. They
//! are public because they are part of this crate's contract — an operator
//! reasoning about behaviour during a provider outage needs both numbers, and
//! a test asserting the security property should move a clock by them rather
//! than by a literal.

/// How soon after *any* attempt another may be made for the same issuer.
///
/// **Amplification protection, and nothing else.** Bounds calls to the issuer
/// whether they are succeeding or failing. Without it, a few thousand invented
/// `kid` values during an outage become a few thousand failing fetches aimed
/// at a provider that is already unwell — the per-issuer lock makes them
/// sequential rather than concurrent, which is no help to the provider.
pub const REFRESH_MIN_INTERVAL_SECONDS: u64 = 10;

/// How long a **successful** snapshot proves an unfamiliar key does not exist.
///
/// **A security semantic, and nothing else.** Shorter than the staleness bound,
/// which governs whether cached keys may still *verify*. This governs
/// something stronger: whether absence from a snapshot is evidence enough to
/// refuse a credential. Claiming a key does not exist is a claim about what
/// the issuer publishes *now*, so it expires quickly.
///
/// A failed attempt never renews it. A failure cannot create negative evidence
/// about a key, so it must never age into grounds for a refusal.
pub const UNKNOWN_KID_FRESHNESS_SECONDS: u64 = 30;
