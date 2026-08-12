//! The two [`TokenReader`](crate::TokenReader) implementations.
//!
//! [`TrustedIngressReader`] is the canonical one and the default: the platform
//! edge authenticates, and the runtime consumes the identity it established
//! (§8, §9).
//!
//! [`ValidatingReader`] adds signature verification as an optional layer of
//! defence in depth. It is not the recommended architecture and not a
//! substitute for the network policy §9 requires.

pub(crate) mod expiry;
pub(crate) mod jwt_payload;

mod jwks;
mod trusted_ingress;
mod unsigned_token;
mod validating;
mod validation_rules;
mod verification_keys;

pub use trusted_ingress::TrustedIngressReader;
pub use unsigned_token::encode_unsigned_token;
pub use validating::ValidatingReader;
pub use verification_keys::VerificationKeys;
