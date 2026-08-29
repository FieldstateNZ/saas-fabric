//! The signing keys an operator token is verified against.
//!
//! # Held in memory, refreshed elsewhere
//!
//! Authenticating an operator happens on every request, and
//! [`Operator`](crate::Operator)'s extractor is deliberately not `async` so
//! that a network call cannot quietly appear in front of every one of them.
//! That constrains this module: verification reads keys already here, and
//! something outside swaps in a newer set.
//!
//! The runtime plane's `TokenReader` made the same call for the same reason.
//! The two do not share code — the planes share only `fabric-core` (ADR 0008)
//! — but they should not disagree, and this comment is the record that the
//! agreement is intentional.

use jsonwebtoken::DecodingKey;
use serde::Deserialize;

/// One usable key and the id it was published under, if any.
pub(super) struct VerificationKey {
    /// The `kid`, when the entry declared one.
    pub(super) key_id: Option<String>,

    /// The key itself.
    pub(super) key: DecodingKey,
}

/// Every key currently trusted to have signed an operator's token.
#[derive(Default)]
pub struct VerificationKeys {
    /// The usable keys, in the order the document published them.
    keys: Vec<VerificationKey>,
}

impl VerificationKeys {
    /// Reads every usable RSA key out of a JWKS document.
    ///
    /// Entries of other key types are **skipped rather than rejected**, so one
    /// exotic key in a provider's document does not stop the platform reading
    /// the rest of them.
    ///
    /// # Errors
    ///
    /// Returns a message if the document is not JSON, has no `keys` array, or
    /// yields no usable key — the last because a key set nothing can be
    /// verified against is a failure worth naming at the moment it is fetched
    /// rather than one `401` at a time afterwards.
    pub fn parse(document: &str) -> Result<Self, String> {
        let parsed: Document = serde_json::from_str(document)
            .map_err(|error| format!("operator key set is not valid JSON: {error}"))?;

        let keys: Vec<VerificationKey> = parsed.keys.iter().filter_map(Entry::read).collect();

        if keys.is_empty() {
            return Err("operator key set contained no usable RSA keys".to_owned());
        }

        Ok(Self { keys })
    }

    /// A key set built directly, for tests that sign their own tokens.
    #[cfg(test)]
    pub(super) fn held(keys: Vec<(Option<&str>, DecodingKey)>) -> Self {
        Self {
            keys: keys
                .into_iter()
                .map(|(key_id, key)| VerificationKey {
                    key_id: key_id.map(str::to_owned),
                    key,
                })
                .collect(),
        }
    }

    /// The keys a token bearing this `kid` could have been signed by.
    ///
    /// A token naming a `kid` is checked against that key alone. One that
    /// names none is checked against all of them, which is the only thing a
    /// verifier can do and is safe: a signature still has to verify.
    pub(super) fn candidates(&self, key_id: Option<&str>) -> Vec<&DecodingKey> {
        let Some(wanted) = key_id else {
            return self.keys.iter().map(|entry| &entry.key).collect();
        };

        self.keys
            .iter()
            .filter(|entry| entry.key_id.as_deref() == Some(wanted))
            .map(|entry| &entry.key)
            .collect()
    }

    /// How many keys are held, for the log line that reports a refresh.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether no key is held — the state before the first refresh lands.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// A JWKS document, as far as this platform reads it.
#[derive(Deserialize)]
struct Document {
    /// The published keys.
    #[serde(default)]
    keys: Vec<Entry>,
}

/// One published key.
#[derive(Deserialize)]
struct Entry {
    /// The key type. Only `RSA` is read.
    kty: String,
    /// The key id, when published.
    #[serde(default)]
    kid: Option<String>,
    /// The RSA modulus.
    #[serde(default)]
    n: Option<String>,
    /// The RSA exponent.
    #[serde(default)]
    e: Option<String>,
}

impl Entry {
    /// Reads this entry, returning `None` for anything unusable.
    fn read(&self) -> Option<VerificationKey> {
        if self.kty != "RSA" {
            return None;
        }

        let key = DecodingKey::from_rsa_components(self.n.as_deref()?, self.e.as_deref()?).ok()?;

        Some(VerificationKey {
            key_id: self.kid.clone(),
            key,
        })
    }
}
