//! Checking one listed referrer: fetched by digest, hashed, and its shape
//! held to a component descriptor's.
//!
//! # Why every candidate is fetched, on both paths
//!
//! The referrers tag schema's index is written by whoever publishes, and a
//! referrers API answer is a registry's summary. Neither is trusted until
//! the manifest it lists has been read by digest, hashed, and checked.

use fabric_platform_management::{RegistryError, Unusable};

use super::shape::check;
use crate::client::digest::sha256;
use crate::client::fetch::Held;
use crate::client::OciRegistry;

/// What one listed referrer turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Candidate {
    /// Its manifest is not served: not attached.
    Missing,

    /// Of the family, and not usable.
    Unusable(Unusable),

    /// A component descriptor's manifest, attached to the subject.
    Usable(Checked),
}

/// A checked component descriptor manifest: what it says, and where its one
/// layer is.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Checked {
    /// The manifest's digest, as computed.
    pub(super) digest: String,

    /// Its `artifactType`, of the family.
    pub(super) artifact_type: String,

    /// Its one layer's digest, `sha256`.
    pub(super) layer_digest: String,

    /// Its one layer's declared size, within the bound.
    pub(super) layer_size: u64,

    /// Its revision annotation.
    pub(super) revision: Option<String>,

    /// Its version annotation.
    pub(super) version: Option<String>,
}

impl OciRegistry {
    /// Fetches and checks one candidate.
    ///
    /// Asked of the registry every time rather than answered from held
    /// bytes: whether the listed manifest exists *now* is part of the answer.
    pub(super) async fn candidate(
        &self,
        repository: &str,
        subject: &str,
        digest: &str,
    ) -> Result<Candidate, RegistryError> {
        if sha256(digest, "a listed referrer").is_err() {
            return Ok(Candidate::Unusable(Unusable::Malformed));
        }

        Ok(
            match self.manifest_by_digest(repository, digest, Held::Ask).await? {
                None => Candidate::Missing,
                Some(content) => check(subject, &content),
            },
        )
    }
}
