//! The public keys a [`ValidatingReader`](crate::ValidatingReader) trusts.

use std::collections::HashMap;

use jsonwebtoken::DecodingKey;

/// The set of public keys a deployment will accept token signatures from,
/// indexed by key id.
///
/// Built from a JWKS document — the format every OIDC provider publishes — so
/// nothing here is specific to any one identity provider (§24). Keycloak,
/// Entra ID, Auth0, and a customer's own broker all serve the same shape.
///
/// # Key rotation
///
/// This type is a *snapshot*. When the provider rotates its keys, build a new
/// `VerificationKeys` and swap in a new reader; do not mutate this one. Fetching
/// a JWKS document is I/O, and I/O does not belong on the request path — the
/// same reasoning that keeps Git out of request handling in §6.
///
/// Publishing the new key set *before* the provider starts signing with it is
/// the operator's job, and is why providers advertise several keys at once.
pub struct VerificationKeys {
    by_key_id: HashMap<String, DecodingKey>,
    /// Used when a token carries no `kid` and exactly one key is configured.
    /// With several keys and no `kid` there is no safe choice, so verification
    /// fails rather than trying each in turn — trying each would turn a
    /// misconfigured provider into a silent accept.
    sole_key: Option<DecodingKey>,
}

impl VerificationKeys {
    /// Parses a JWKS document.
    ///
    /// Only RSA keys are read. That covers what the mainstream OIDC providers
    /// issue by default; entries of other key types are skipped rather than
    /// failing the whole document, so one exotic key in a provider's JWKS does
    /// not take the platform down.
    ///
    /// # Errors
    ///
    /// Returns a message if the document is not valid JSON, has no `keys`
    /// array, or yields no usable RSA key.
    pub fn from_jwks_json(document: &str) -> Result<Self, String> {
        let parsed: serde_json::Value =
            serde_json::from_str(document).map_err(|error| format!("JWKS is not valid JSON: {error}"))?;

        let entries = parsed
            .get("keys")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "JWKS has no \"keys\" array".to_owned())?;

        let mut by_key_id = HashMap::new();
        let mut usable = Vec::new();

        for entry in entries {
            if entry.get("kty").and_then(serde_json::Value::as_str) != Some("RSA") {
                continue;
            }

            let (Some(modulus), Some(exponent)) = (
                entry.get("n").and_then(serde_json::Value::as_str),
                entry.get("e").and_then(serde_json::Value::as_str),
            ) else {
                continue;
            };

            let Ok(key) = DecodingKey::from_rsa_components(modulus, exponent) else {
                continue;
            };

            if let Some(key_id) = entry.get("kid").and_then(serde_json::Value::as_str) {
                by_key_id.insert(key_id.to_owned(), key.clone());
            }

            usable.push(key);
        }

        if usable.is_empty() {
            return Err("JWKS contained no usable RSA keys".to_owned());
        }

        let sole_key = if usable.len() == 1 {
            usable.into_iter().next()
        } else {
            None
        };

        Ok(Self { by_key_id, sole_key })
    }

    /// Builds a key set from a single PEM-encoded RSA public key.
    ///
    /// Useful for deployments that pin one key in configuration rather than
    /// fetching a JWKS document.
    ///
    /// # Errors
    ///
    /// Returns a message if the PEM is not a readable RSA public key.
    pub fn from_rsa_pem(pem: &[u8]) -> Result<Self, String> {
        let key =
            DecodingKey::from_rsa_pem(pem).map_err(|error| format!("not a valid RSA public key: {error}"))?;

        Ok(Self {
            by_key_id: HashMap::new(),
            sole_key: Some(key),
        })
    }

    /// Selects the key to verify a token with.
    ///
    /// Returns `None` when the `kid` is unknown, or when the token has no `kid`
    /// and more than one key is configured.
    pub(crate) fn select(&self, key_id: Option<&str>) -> Option<&DecodingKey> {
        match key_id {
            Some(id) => self.by_key_id.get(id),
            None => self.sole_key.as_ref(),
        }
    }

    /// How many distinct keys were loaded, for startup logging.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key_id.len().max(usize::from(self.sole_key.is_some()))
    }

    /// Whether the set is empty. Never true for a successfully built set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A syntactically valid RSA JWKS entry. The key material is a real
    /// 2048-bit modulus so `from_rsa_components` accepts it; nothing signs with
    /// it.
    const MODULUS: &str = "sXchDaQebHnPiGvyDOAT4saGEUetSyo9MKLOoWFsueri23bOdgWp4Dy1Wl\
UzewbgBHod5pcM9H95GQRV3JDXboIRROSBigeC5yjU1hGzHHyXss8UDpre\
cbAYxknTcQkhslANGRUZmdTOQ5qTRsLAt6BTYuyvVRdhS8exSZEy_c4gs_\
7svlJJQ4H9_NxsiIoLwAEk7-Q3UXERGYw_75IDrGA84-lA_-Ct4eTlXHBI\
Y2EaV7t7LjJaynVJCpkv4LKjTTAumiGUIuQhrNhZLuF_RJLqHpM2kgWFLU\
7-VTdL1VbC2tejvcI2BlMkEpk1BzBZI0KQB0GaDWFLN-aEAw3vRw";

    fn jwks_with(kid: &str) -> String {
        format!(r#"{{"keys":[{{"kty":"RSA","kid":"{kid}","n":"{MODULUS}","e":"AQAB"}}]}}"#)
    }

    #[test]
    fn parses_a_single_rsa_key_and_selects_it_by_id() {
        let keys = VerificationKeys::from_jwks_json(&jwks_with("key-1")).unwrap();
        assert_eq!(keys.len(), 1);
        assert!(keys.select(Some("key-1")).is_some());
    }

    #[test]
    fn a_single_key_is_used_when_the_token_carries_no_key_id() {
        let keys = VerificationKeys::from_jwks_json(&jwks_with("key-1")).unwrap();
        assert!(keys.select(None).is_some());
    }

    #[test]
    fn an_unknown_key_id_selects_nothing() {
        let keys = VerificationKeys::from_jwks_json(&jwks_with("key-1")).unwrap();
        assert!(keys.select(Some("key-2")).is_none());
    }

    #[test]
    fn with_several_keys_and_no_key_id_verification_has_no_safe_choice() {
        let document = format!(
            r#"{{"keys":[
                {{"kty":"RSA","kid":"a","n":"{MODULUS}","e":"AQAB"}},
                {{"kty":"RSA","kid":"b","n":"{MODULUS}","e":"AQAB"}}
            ]}}"#
        );
        let keys = VerificationKeys::from_jwks_json(&document).unwrap();

        assert_eq!(keys.len(), 2);
        assert!(keys.select(None).is_none());
    }

    #[test]
    fn non_rsa_entries_are_skipped_rather_than_failing_the_document() {
        let document = format!(
            r#"{{"keys":[
                {{"kty":"OKP","kid":"ed","crv":"Ed25519","x":"abc"}},
                {{"kty":"RSA","kid":"rsa","n":"{MODULUS}","e":"AQAB"}}
            ]}}"#
        );
        let keys = VerificationKeys::from_jwks_json(&document).unwrap();

        assert!(keys.select(Some("rsa")).is_some());
    }

    #[test]
    fn a_document_with_no_usable_key_is_an_error() {
        let document = r#"{"keys":[{"kty":"OKP","kid":"ed","crv":"Ed25519","x":"abc"}]}"#;
        assert!(VerificationKeys::from_jwks_json(document).is_err());
    }

    #[test]
    fn a_document_without_a_keys_array_is_an_error() {
        assert!(VerificationKeys::from_jwks_json("{}").is_err());
    }
}
