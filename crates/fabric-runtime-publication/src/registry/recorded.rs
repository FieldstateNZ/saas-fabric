//! What the publication record says about the registry.

use super::RegistryGeneration;

/// The registry generation a publication record names, and the digest of
/// its bytes exactly as published.
///
/// Recorded beside the three documents so a reader holding the record can
/// tell whether the registry it reads belongs to the same generation as the
/// tenants document it was generated from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedRegistry {
    /// The registry's generation.
    pub generation: RegistryGeneration,
    /// The lowercase hex SHA-256 of `issuer-registry.json` as published.
    pub sha256: String,
}
