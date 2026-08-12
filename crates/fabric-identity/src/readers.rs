//! The two [`TokenReader`](crate::TokenReader) implementations.
//!
//! They differ in exactly one respect — whether the signature is checked — and
//! that one respect is the difference between §11 being enforceable and being
//! aspirational. Read both type-level docs before choosing.

mod trusted_ingress;
mod validating;
mod verification_keys;

pub use trusted_ingress::{encode_unsigned_token, TrustedIngressReader};
pub use validating::ValidatingReader;
pub use verification_keys::VerificationKeys;
