//! What one read of the published documents yields.

use async_trait::async_trait;

use super::LastKnownGoodState;
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// The tenants and data sources one read found, and whether they are known
/// to belong together.
#[derive(Debug)]
pub enum PublishedState {
    /// Both documents are exactly the bytes the publication record names:
    /// one complete generation, to be applied whole.
    Generation {
        /// The record's generation number.
        generation: u64,
        /// The SHA-256 of the record's exact bytes, which identifies the
        /// generation exactly.
        record_sha256: fabric_core::RecordDigest,
        /// Whether it was read from the published files or from this
        /// replica's persisted last-known-good copy.
        origin: GenerationOrigin,
        /// The data-sources document.
        data_sources: Vec<DataSource>,
        /// The tenants document.
        tenants: Vec<TenantRuntimeBinding>,
    },
    /// No publication record exists: the documents were written by a
    /// publisher older than the record, or by hand. Nothing says which
    /// revisions belong together, so each is applied on its own, as every
    /// reader did before the record existed.
    Unrecorded {
        /// The data-sources document, or why it could not be loaded.
        data_sources: Result<Vec<DataSource>, SourceError>,
        /// The tenants document, or why it could not be loaded.
        tenants: Result<Vec<TenantRuntimeBinding>, SourceError>,
    },
}

/// Where a complete generation was read from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum GenerationOrigin {
    /// The published files, matching their record.
    Published,
    /// The persisted last-known-good copy, because no published generation
    /// could be applied at start. The replica holds it until a published
    /// generation at or above it validates.
    LastKnownGood {
        /// Why the published files were not used.
        held_because: String,
    },
}

/// Supplies the published tenants and data sources together.
///
/// The same rules as [`ResourceSource`](crate::ResourceSource): read only
/// what reconciliation already produced, and never turn a failure into an
/// empty set. An `Err` is a generation that cannot be applied at all --
/// incomplete, refused by its floor, or unreadable -- and leaves both
/// registries as they were.
#[async_trait]
pub trait PublishedStateSource: Send + Sync {
    /// Reads both documents once.
    ///
    /// # Errors
    ///
    /// [`SourceError`] when no generation can be applied.
    async fn load(&self) -> Result<PublishedState, SourceError>;

    /// [`Self::load`] at process start, with the catalogue's bytes when this
    /// source reads one: taken in the same read as the documents, and, for a
    /// recorded generation, checked against the same record. A runtime that
    /// builds its catalogue from these bytes serves the catalogue of the
    /// generation it primed, not one published between the two reads (G8).
    ///
    /// # Errors
    ///
    /// As [`Self::load`], and for a recorded generation
    /// [`SourceError::IncompleteGeneration`] naming `catalog` when the
    /// catalogue is not the record's.
    async fn load_at_start(&self) -> Result<(PublishedState, Option<Vec<u8>>), SourceError> {
        Ok((self.load().await?, None))
    }

    /// A short description for logging.
    fn describe(&self) -> String;

    /// What this source's last-known-good copy holds, for readiness.
    /// [`LastKnownGoodState::Disabled`] unless the source persists one.
    fn last_known_good(&self) -> LastKnownGoodState {
        LastKnownGoodState::Disabled
    }
}
