//! The sidecar manifest published beside every document, and the file names
//! both halves are written under.

use crate::canonical::to_canonical_bytes;
use crate::DocumentRevision;

/// The contract version every manifest states.
///
/// The runtime does not read this field, and does not need to: an
/// incompatible *shape* already fails loudly through `deny_unknown_fields`
/// on the consumer's own types. What this buys is the migration path for a
/// change of *meaning* at an unchanged shape — a breaking change ships as new
/// file names alongside the old ones, never as a silent reinterpretation of a
/// version already published.
pub const CONTRACT_VERSION: u32 = 1;

/// Which of the three documents a manifest describes.
///
/// Serialised as the document's own name, so the value in
/// [`DocumentManifest::document`] and the file the manifest sits beside
/// always say the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    /// Describes `tenants.json`.
    Tenants,
    /// Describes `data-sources.json`.
    DataSources,
    /// Describes `catalog.json`.
    Catalog,
}

impl DocumentKind {
    /// This document's payload file name, such as `tenants.json`.
    ///
    /// The one place either the filesystem adapter or a caller building path
    /// names should reach for this — never a hand-written literal, so a
    /// document and the file it is written under cannot drift apart.
    #[must_use]
    pub const fn payload_file(self) -> &'static str {
        match self {
            Self::Tenants => TENANTS_FILE,
            Self::DataSources => DATA_SOURCES_FILE,
            Self::Catalog => CATALOG_FILE,
        }
    }

    /// This document's manifest file name, such as `tenants.manifest.json`.
    #[must_use]
    pub const fn manifest_file(self) -> &'static str {
        match self {
            Self::Tenants => TENANTS_MANIFEST_FILE,
            Self::DataSources => DATA_SOURCES_MANIFEST_FILE,
            Self::Catalog => CATALOG_MANIFEST_FILE,
        }
    }
}

/// The sidecar that travels beside every published document.
///
/// Deliberately three fields and no more — in particular, no timestamp:
/// nothing branches on one, and a timestamp in a ConfigMap is a diff that
/// churns on every publication for no reader's benefit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentManifest {
    /// The contract version this manifest was written under.
    pub contract_version: u32,
    /// Which document this manifest describes.
    pub document: DocumentKind,
    /// The document's own revision — see [`DocumentRevision`] for why this is
    /// never a resource's revision.
    pub revision: DocumentRevision,
}

impl DocumentManifest {
    /// Builds a manifest for `document` at `revision`, stamped with this
    /// crate's own [`CONTRACT_VERSION`] — the one place that stamp happens,
    /// so a manifest can never be built under the wrong version by accident.
    #[must_use]
    pub const fn new(document: DocumentKind, revision: DocumentRevision) -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            document,
            revision,
        }
    }

    /// Renders this manifest as canonical JSON (two-space indentation, a
    /// trailing newline, UTF-8) — the same formatting rule every published
    /// document shares.
    ///
    /// # Errors
    ///
    /// Returns [`serde_json::Error`] only if this type's own `Serialize`
    /// implementation fails, which it never does.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        to_canonical_bytes(self)
    }
}

/// The `tenants.json` payload file name.
pub const TENANTS_FILE: &str = "tenants.json";
/// The `tenants.json` manifest file name.
pub const TENANTS_MANIFEST_FILE: &str = "tenants.manifest.json";
/// The `data-sources.json` payload file name.
pub const DATA_SOURCES_FILE: &str = "data-sources.json";
/// The `data-sources.json` manifest file name.
pub const DATA_SOURCES_MANIFEST_FILE: &str = "data-sources.manifest.json";
/// The `catalog.json` payload file name.
pub const CATALOG_FILE: &str = "catalog.json";
/// The `catalog.json` manifest file name.
pub const CATALOG_MANIFEST_FILE: &str = "catalog.manifest.json";

#[cfg(test)]
mod tests {
    use super::*;

    /// A ConfigMap `data` key must match `[-._a-zA-Z0-9]+` — every payload
    /// and manifest file name becomes exactly such a key in production
    /// (ADR 0018, "The production owner").
    fn is_valid_configmap_key(key: &str) -> bool {
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
    }

    #[test]
    fn every_document_key_is_valid_configmap_data() {
        for key in [
            TENANTS_FILE,
            TENANTS_MANIFEST_FILE,
            DATA_SOURCES_FILE,
            DATA_SOURCES_MANIFEST_FILE,
            CATALOG_FILE,
            CATALOG_MANIFEST_FILE,
        ] {
            assert!(is_valid_configmap_key(key), "{key}");
        }
    }

    #[test]
    fn a_manifest_serialises_the_document_kind_as_its_wire_name() {
        let manifest = DocumentManifest::new(DocumentKind::DataSources, DocumentRevision::new(3));

        let json = serde_json::to_string(&manifest).unwrap();

        assert!(json.contains(r#""document":"data-sources""#), "{json}");
    }

    #[test]
    fn new_stamps_the_current_contract_version() {
        let manifest = DocumentManifest::new(DocumentKind::Catalog, DocumentRevision::new(1));

        assert_eq!(manifest.contract_version, CONTRACT_VERSION);
    }

    #[test]
    fn each_kind_names_its_own_payload_and_manifest_files() {
        assert_eq!(DocumentKind::Tenants.payload_file(), TENANTS_FILE);
        assert_eq!(DocumentKind::Tenants.manifest_file(), TENANTS_MANIFEST_FILE);
        assert_eq!(DocumentKind::DataSources.payload_file(), DATA_SOURCES_FILE);
        assert_eq!(
            DocumentKind::DataSources.manifest_file(),
            DATA_SOURCES_MANIFEST_FILE
        );
        assert_eq!(DocumentKind::Catalog.payload_file(), CATALOG_FILE);
        assert_eq!(DocumentKind::Catalog.manifest_file(), CATALOG_MANIFEST_FILE);
    }

    #[test]
    fn canonical_json_ends_in_a_trailing_newline() {
        let manifest = DocumentManifest::new(DocumentKind::Tenants, DocumentRevision::new(1));

        let bytes = manifest.canonical_json().unwrap();

        assert!(bytes.ends_with(b"\n"), "{}", String::from_utf8_lossy(&bytes));
    }
}
