//! Decodes the tokens `fabric_identity::encode_unsigned_token` builds,
//! without a signature or an expiry check.
//!
//! # Why this crate cannot just use `TrustedIngressReader`
//!
//! `fabric_identity::TrustedIngressReader::new` takes an `Arc<dyn
//! fabric_core::Clock>`, and this crate deliberately has no `fabric-core`
//! dependency of its own -- `scripts/check_architecture.py`'s
//! dependency-direction table does not list one for `fabric-ndc-acceptance`,
//! and this issue does not touch `scripts/` to add one (see
//! `fixtures.rs`'s module doc for the same constraint applied to the
//! publication fixture). Implementing `fabric_core::Clock` for a local type
//! needs the trait in scope, which needs the crate.
//!
//! So this reader exists to decode exactly the wire format
//! `encode_unsigned_token` produces -- `base64url(header).base64url(payload
//! JSON).signature-not-verified` -- reading only the payload segment, the
//! same thing `TrustedIngressReader` does before it goes on to check `exp`
//! and `nbf` against a clock. Every claim set this suite mints is fresh and
//! carries neither, so that half of `TrustedIngressReader`'s behaviour is
//! never exercised by omitting it here; it is `fabric-identity`'s own test
//! suite's job to prove, not this crate's, which is about tenant isolation
//! and the NDC adapter, not clock-skew handling.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use fabric_identity::{IdentityError, TokenClaims, TokenReader};
use serde_json::Value;

/// A [`TokenReader`] for this suite's own unsigned tokens only. Never a
/// production posture -- see the module doc.
pub struct UnsignedTokenReader;

impl TokenReader for UnsignedTokenReader {
    fn read(&self, token: &str) -> Result<TokenClaims, IdentityError> {
        let payload = token.split('.').nth(1).ok_or(IdentityError::MalformedToken)?;

        let decoded = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| IdentityError::MalformedToken)?;

        match serde_json::from_slice(&decoded) {
            Ok(Value::Object(claims)) => Ok(TokenClaims::new(claims)),
            _ => Err(IdentityError::MalformedToken),
        }
    }

    fn describe(&self) -> &'static str {
        "fabric-ndc-acceptance UnsignedTokenReader (test-only; no signature, no clock)"
    }
}
