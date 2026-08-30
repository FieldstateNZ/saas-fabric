//! The signing keys an issuer publishes, and where they come from.

use std::collections::BTreeMap;

use async_trait::async_trait;
use jsonwebtoken::DecodingKey;
use serde::Deserialize;

/// Every usable key one issuer currently publishes, by `kid`.
///
/// Keys published without a `kid` are dropped rather than kept under a
/// sentinel. This crate selects a key *by* the token's `kid`, so a key that
/// cannot be selected is a key that would only ever be tried by guessing —
/// and "try every key until one verifies" is how an implementation ends up
/// accepting a signature from a key the issuer retired.
#[derive(Default)]
pub struct KeySet {
    /// Decoding keys by the id they were published under.
    keys: BTreeMap<String, DecodingKey>,
}

impl KeySet {
    /// Reads the usable RSA keys out of a JWKS document.
    ///
    /// # Errors
    ///
    /// Returns the parse failure as a message. An entry this crate cannot use
    /// is skipped rather than fatal — one exotic key must not stop the rest of
    /// a provider's document being read — but a document that will not parse
    /// at all is a failure to establish trust.
    pub fn from_jwks(document: &str) -> Result<Self, String> {
        let parsed: JwksDocument =
            serde_json::from_str(document).map_err(|error| format!("jwks: {error}"))?;

        let mut keys = BTreeMap::new();

        for key in parsed.keys {
            let (Some(kid), Some(n), Some(e)) = (key.kid, key.n, key.e) else {
                continue;
            };

            if key.kty != "RSA" {
                continue;
            }

            if let Ok(decoding) = DecodingKey::from_rsa_components(&n, &e) {
                keys.insert(kid, decoding);
            }
        }

        Ok(Self { keys })
    }

    /// Builds a set directly from decoding keys.
    ///
    /// `pub(crate)` and used by this crate's tests, which sign with a symmetric
    /// key rather than embedding an RSA private key in the repository — the
    /// same choice the control plane's OIDC tests made, for the same reason.
    #[cfg(test)]
    pub(crate) fn from_entries(entries: impl IntoIterator<Item = (String, DecodingKey)>) -> Self {
        Self {
            keys: entries.into_iter().collect(),
        }
    }

    /// The key published under this id, if any.
    #[must_use]
    pub fn get(&self, key_id: &str) -> Option<&DecodingKey> {
        self.keys.get(key_id)
    }

    /// Whether this set publishes the id.
    #[must_use]
    pub fn contains(&self, key_id: &str) -> bool {
        self.keys.contains_key(key_id)
    }

    /// How many usable keys the set holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the set holds no usable key.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Where a key set is read from.
///
/// A port rather than a `reqwest` call, so the rotation rules can be driven
/// deterministically in tests: a fetch that fails, a fetch that succeeds
/// without the key, and a fetch that succeeds with it are three different
/// outcomes and each has to be exercised.
#[async_trait]
pub trait KeySource: Send + Sync {
    /// Reads the key set published at this address.
    ///
    /// # Errors
    ///
    /// Returns a message when the keys could not be read. Every failure is an
    /// availability failure: this port cannot refuse a credential, only fail
    /// to establish trust.
    async fn fetch(&self, jwks_uri: &str) -> Result<KeySet, String>;
}

/// A JWKS document, as far as this crate reads one.
#[derive(Deserialize)]
struct JwksDocument {
    /// The published keys.
    keys: Vec<JwkEntry>,
}

/// One entry in a JWKS document.
#[derive(Deserialize)]
struct JwkEntry {
    /// The key type; only `RSA` is used here.
    kty: String,
    /// The id this key is selected by.
    kid: Option<String>,
    /// RSA modulus, base64url.
    n: Option<String>,
    /// RSA exponent, base64url.
    e: Option<String>,
}
