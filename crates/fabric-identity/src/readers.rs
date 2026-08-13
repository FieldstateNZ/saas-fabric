//! The two [`TokenReader`](crate::TokenReader) implementations.
//!
//! [`TrustedIngressReader`] is the canonical one and the default: the platform
//! edge authenticates, and the runtime consumes the identity it established
//! (§8, §9).
//!
//! [`ValidatingReader`] adds signature verification as an optional layer of
//! defence in depth. It is not the recommended architecture and not a
//! substitute for the network policy §9 requires.
//!
//! The two must agree about *when* a token is valid even though they check it
//! by different means, so the validity window lives in shared pieces —
//! [`LeewaySeconds`], `expiry`, `not_before`, `rejection` — and
//! `posture_parity_tests` holds the pair against each other.

pub(crate) mod expiry;
pub(crate) mod jwt_payload;
pub(crate) mod not_before;
pub(crate) mod rejection;

mod jwks;
mod leeway;
mod trusted_ingress;
mod unsigned_token;
mod validating;
mod validation_rules;
mod verification_keys;

#[cfg(test)]
mod posture_parity_tests;

pub use leeway::LeewaySeconds;
pub use trusted_ingress::TrustedIngressReader;
pub use unsigned_token::encode_unsigned_token;
pub use validating::ValidatingReader;
pub use verification_keys::VerificationKeys;
