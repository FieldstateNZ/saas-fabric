//! The public keys a [`ValidatingReader`](crate::ValidatingReader) trusts.

use std::collections::HashMap;

use jsonwebtoken::DecodingKey;

use crate::readers::jwks;

/// The keys a deployment will accept token signatures from, indexed by key id.
///
/// Built from a JWKS document — the format every OIDC provider publishes — so
/// nothing here is specific to any one provider (§24).
///
/// # A snapshot, deliberately
///
/// This never fetches anything. Key material is loaded once, outside the
/// request path, and rotation means building a new set and swapping the reader.
///
/// That is not a limitation to be fixed by adding a fetcher: JWKS lifecycle is
/// an identity-provider responsibility, and this crate only takes on as much of
/// it as a deployment explicitly opts into by choosing defence-in-depth
/// verification at all.
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
    /// # Errors
    ///
    /// Returns a message if the document is unreadable or yields no usable RSA
    /// key.
    pub fn from_jwks_json(document: &str) -> Result<Self, String> {
        let parsed = jwks::parse(document)?;

        let by_key_id = parsed
            .iter()
            .filter_map(|entry| entry.key_id.clone().map(|id| (id, entry.key.clone())))
            .collect();

        // A lone key is usable without a `kid`; several are not.
        let sole_key = match parsed.len() {
            1 => parsed.into_iter().next().map(|entry| entry.key),
            _ => None,
        };

        Ok(Self { by_key_id, sole_key })
    }

    /// Builds a key set from a single PEM-encoded RSA public key.
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
    /// Returns `None` for an unknown `kid`, or when a token carries no `kid`
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
pub(crate) mod tests {
    use super::*;

    /// Test-only constructors. Deliberately not on the public surface: a shared
    /// secret is an HMAC key, and this crate verifies with public keys only.
    impl VerificationKeys {
        /// Wraps a symmetric secret as the sole key.
        ///
        /// Used by `posture_parity_tests`, whose fixtures are HS256 because the
        /// signature is not what those tests are about.
        pub(crate) fn from_shared_secret(secret: &[u8]) -> Self {
            Self {
                by_key_id: HashMap::new(),
                sole_key: Some(DecodingKey::from_secret(secret)),
            }
        }
    }

    /// A real 2048-bit modulus, so `from_rsa_components` accepts it. Nothing
    /// signs with it.
    pub(crate) const MODULUS: &str = "sXchDaQebHnPiGvyDOAT4saGEUetSyo9MKLOoWFsueri23bOdgWp4Dy1Wl\
UzewbgBHod5pcM9H95GQRV3JDXboIRROSBigeC5yjU1hGzHHyXss8UDpre\
cbAYxknTcQkhslANGRUZmdTOQ5qTRsLAt6BTYuyvVRdhS8exSZEy_c4gs_\
7svlJJQ4H9_NxsiIoLwAEk7-Q3UXERGYw_75IDrGA84-lA_-Ct4eTlXHBI\
Y2EaV7t7LjJaynVJCpkv4LKjTTAumiGUIuQhrNhZLuF_RJLqHpM2kgWFLU\
7-VTdL1VbC2tejvcI2BlMkEpk1BzBZI0KQB0GaDWFLN-aEAw3vRw";

    pub(crate) fn jwks_with(key_id: &str) -> String {
        format!(r#"{{"keys":[{{"kty":"RSA","kid":"{key_id}","n":"{MODULUS}","e":"AQAB"}}]}}"#)
    }

    #[test]
    fn selects_a_key_by_its_id() {
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
    fn with_several_keys_and_no_key_id_there_is_no_safe_choice() {
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
}
