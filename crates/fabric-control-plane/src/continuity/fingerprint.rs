//! A client's lifecycle, bound to its identity context, named by the SHA-256
//! of its canonical JSON.

use fabric_client_model::{ClientDocument, ClientId, LifecycleRecord, RealmName};

/// What the continuity store compares a document with: the lifecycle
/// record, its identity instance, and the identity context it belongs to
/// (the client id and its realm).
///
/// The canonical form is the serialisation of those three, whose field
/// order is fixed, so equal inputs always fingerprint alike, and a changed,
/// added or erased entry, another instance or another realm never does.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LifecycleFingerprint(String);

#[derive(serde::Serialize)]
struct Canonical<'a> {
    client: &'a str,
    realm: &'a str,
    lifecycle: &'a LifecycleRecord,
}

impl LifecycleFingerprint {
    /// The fingerprint of `lifecycle` for `client` in `realm`.
    #[must_use]
    pub fn of(client: &ClientId, realm: &RealmName, lifecycle: &LifecycleRecord) -> Self {
        let canonical = Canonical {
            client: client.as_str(),
            realm: realm.as_str(),
            lifecycle,
        };
        // Strings, integers and validated names always serialise.
        let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
        let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
        Self(digest.as_ref().iter().fold(String::new(), |mut text, byte| {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        }))
    }

    /// The fingerprint of what `document` records now.
    #[must_use]
    pub fn of_document(document: &ClientDocument) -> Self {
        let client = document.client();
        Self::of(&client.id, &client.identity.realm, &client.lifecycle)
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
