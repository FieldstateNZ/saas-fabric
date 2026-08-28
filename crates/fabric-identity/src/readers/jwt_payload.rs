//! Decoding the payload segment of a JWT.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::Value;

use crate::{IdentityError, TokenClaims};

/// Decodes a JWT's claims without verifying its signature.
///
/// Requires exactly three segments. A two-segment value is not an unsigned JWT
/// to be tolerated — it is malformed, and accepting it would mean accepting
/// something no issuer produced.
///
/// # Errors
///
/// [`IdentityError::MalformedToken`] if the token is not three segments, the
/// payload is not base64url, or it does not decode to a JSON object.
pub(crate) fn decode_payload(token: &str) -> Result<TokenClaims, IdentityError> {
    let payload = payload_segment(token)?;

    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| IdentityError::MalformedToken)?;

    let value: Value = serde_json::from_slice(&decoded).map_err(|_| IdentityError::MalformedToken)?;

    match value {
        Value::Object(object) => Ok(TokenClaims::new(object)),
        _ => Err(IdentityError::MalformedToken),
    }
}

/// Returns the payload segment of a three-part JWT.
fn payload_segment(token: &str) -> Result<&str, IdentityError> {
    let mut segments = token.split('.');

    match (segments.next(), segments.next(), segments.next(), segments.next()) {
        (Some(_header), Some(payload), Some(_signature), None) if !payload.is_empty() => Ok(payload),
        _ => Err(IdentityError::MalformedToken),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_claims_of_a_well_formed_token() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"tenant_id":"acme"}"#);

        let claims = decode_payload(&format!("header.{payload}.signature")).unwrap();

        assert_eq!(claims.string("tenant_id"), Some("acme"));
    }

    #[test]
    fn rejects_a_token_that_is_not_three_segments() {
        assert_eq!(
            decode_payload("only.two").unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn rejects_an_empty_payload_segment() {
        assert_eq!(
            decode_payload("header..signature").unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn rejects_a_payload_that_is_not_base64() {
        assert_eq!(
            decode_payload("header.!!!not-base64!!!.signature").unwrap_err(),
            IdentityError::MalformedToken
        );
    }

    #[test]
    fn rejects_a_payload_that_is_json_but_not_an_object() {
        let payload = URL_SAFE_NO_PAD.encode("[1,2,3]");

        assert_eq!(
            decode_payload(&format!("h.{payload}.s")).unwrap_err(),
            IdentityError::MalformedToken
        );
    }
}
