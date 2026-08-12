//! Reading RSA keys out of a JWKS document.

use jsonwebtoken::DecodingKey;
use serde_json::Value;

/// One usable key and the id it was published under, if any.
pub(crate) struct ParsedKey {
    /// The `kid`, when the entry declared one.
    pub(crate) key_id: Option<String>,
    /// The decoding key.
    pub(crate) key: DecodingKey,
}

/// Extracts every usable RSA key from a JWKS document.
///
/// Entries of other key types are **skipped rather than rejected**, so one
/// exotic key in a provider's JWKS does not stop the platform reading the rest.
/// Only RSA is read, which covers what the mainstream OIDC providers issue.
///
/// # Errors
///
/// Returns a message if the document is not valid JSON, has no `keys` array, or
/// yields no usable key.
pub(crate) fn parse(document: &str) -> Result<Vec<ParsedKey>, String> {
    let parsed: Value =
        serde_json::from_str(document).map_err(|error| format!("JWKS is not valid JSON: {error}"))?;

    let entries = parsed
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| "JWKS has no \"keys\" array".to_owned())?;

    let keys: Vec<ParsedKey> = entries.iter().filter_map(parse_entry).collect();

    if keys.is_empty() {
        return Err("JWKS contained no usable RSA keys".to_owned());
    }

    Ok(keys)
}

/// Reads one JWKS entry, returning `None` for anything unusable.
fn parse_entry(entry: &Value) -> Option<ParsedKey> {
    if entry.get("kty").and_then(Value::as_str) != Some("RSA") {
        return None;
    }

    let modulus = entry.get("n").and_then(Value::as_str)?;
    let exponent = entry.get("e").and_then(Value::as_str)?;
    let key = DecodingKey::from_rsa_components(modulus, exponent).ok()?;

    Some(ParsedKey {
        key_id: entry.get("kid").and_then(Value::as_str).map(ToOwned::to_owned),
        key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::verification_keys::tests::{jwks_with, MODULUS};

    #[test]
    fn reads_a_single_rsa_key() {
        let keys = parse(&jwks_with("key-1")).unwrap();

        assert_eq!(keys.len(), 1);
        assert_eq!(keys.first().unwrap().key_id.as_deref(), Some("key-1"));
    }

    #[test]
    fn skips_non_rsa_entries_rather_than_failing_the_document() {
        let document = format!(
            r#"{{"keys":[
                {{"kty":"OKP","kid":"ed","crv":"Ed25519","x":"abc"}},
                {{"kty":"RSA","kid":"rsa","n":"{MODULUS}","e":"AQAB"}}
            ]}}"#
        );

        assert_eq!(parse(&document).unwrap().len(), 1);
    }

    #[test]
    fn a_document_with_no_usable_key_is_an_error() {
        let document = r#"{"keys":[{"kty":"OKP","kid":"ed","crv":"Ed25519","x":"abc"}]}"#;

        assert!(parse(document).is_err());
    }

    #[test]
    fn a_document_without_a_keys_array_is_an_error() {
        assert!(parse("{}").is_err());
    }

    #[test]
    fn invalid_json_is_an_error() {
        assert!(parse("{not json").is_err());
    }
}
