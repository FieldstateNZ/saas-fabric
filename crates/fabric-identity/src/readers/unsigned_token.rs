//! A token builder for tests elsewhere in the workspace.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::{Map, Value};

/// Builds an unsigned but well-formed JWT from a claim set.
///
/// Exported rather than kept in a test module because the Data API's
/// integration tests need it too, and two copies of base64 assembly is how the
/// two copies drift apart.
///
/// The signature segment is a fixed placeholder. A token from this function is
/// accepted by [`TrustedIngressReader`](crate::TrustedIngressReader) and
/// rejected by [`ValidatingReader`](crate::ValidatingReader), which is exactly
/// the difference between the two postures.
#[must_use]
pub fn encode_unsigned_token(claims: &Map<String, Value>) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(Value::Object(claims.clone()).to_string());

    format!("{header}.{payload}.signature-not-verified")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_three_segment_token() {
        let token = encode_unsigned_token(&Map::new());

        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn round_trips_through_the_payload_decoder() {
        let claims = serde_json::from_str(r#"{"tenant_id":"acme"}"#).unwrap();

        let decoded = crate::readers::jwt_payload::decode_payload(&encode_unsigned_token(&claims)).unwrap();

        assert_eq!(decoded.string("tenant_id"), Some("acme"));
    }
}
