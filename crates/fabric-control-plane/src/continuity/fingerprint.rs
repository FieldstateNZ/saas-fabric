//! A lifecycle record named by the SHA-256 of its canonical JSON.

use fabric_client_model::LifecycleRecord;

/// What the continuity store compares a document's lifecycle with.
///
/// The canonical form is the record's own serialisation, whose field order
/// is fixed by its type, so two equal records always fingerprint alike and
/// a changed, added or erased entry never does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifecycleFingerprint(String);

impl LifecycleFingerprint {
    /// The fingerprint of `record`.
    #[must_use]
    pub fn of(record: &LifecycleRecord) -> Self {
        // A record of strings and integers always serialises.
        let canonical = serde_json::to_vec(record).unwrap_or_default();
        let digest = ring::digest::digest(&ring::digest::SHA256, &canonical);
        Self(digest.as_ref().iter().fold(String::new(), |mut text, byte| {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        }))
    }

    /// The hexadecimal digest.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LifecycleFingerprint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
