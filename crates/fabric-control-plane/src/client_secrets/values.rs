//! What a secret holds, and what may be said about it without revealing it.

use std::collections::BTreeMap;

/// What is known about a secret without reading it.
///
/// Key **names**, never values. A console lists secrets constantly and reveals
/// one rarely, so the operation it performs constantly is the one that cannot
/// leak.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SecretMetadata {
    /// The version currently stored.
    pub version: u64,

    /// The names of the keys it holds.
    pub keys: Vec<String>,

    /// When it was last written, as the store reported it.
    pub updated_at: Option<String>,
}

/// A secret's actual values.
///
/// # Debug is written by hand
///
/// The derive would print every value the first time this type reached a log
/// line, a panic message, or an error rendered with `{:?}`. That is how a
/// secret leaks — not by anybody deciding to print it. `Debug` names the keys
/// and redacts the values, which is what somebody debugging actually needs.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretValues(BTreeMap<String, String>);

impl SecretValues {
    /// Builds a set of values.
    #[must_use]
    pub fn new(values: BTreeMap<String, String>) -> Self {
        Self(values)
    }

    /// The key names, which are safe to show.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// The values, for the one caller that deliberately asked for them.
    #[must_use]
    pub const fn revealed(&self) -> &BTreeMap<String, String> {
        &self.0
    }

    /// Whether there is nothing here.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for SecretValues {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretValues")
            .field("keys", &self.keys())
            .field("values", &"redacted")
            .finish()
    }
}
