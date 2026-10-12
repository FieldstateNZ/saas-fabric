//! What one read of the published documents yields.

use async_trait::async_trait;

use fabric_core::RecordDigest;

use super::{DecodedCatalogue, LastKnownGoodState};
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// The tenants, data sources and catalogue one read found, and whether they
/// are known to belong together.
#[derive(Debug)]
pub enum PublishedState {
    /// All three documents are exactly the bytes the publication record
    /// names, and the catalogue decodes: one complete generation, to be
    /// installed whole or not at all.
    Generation {
        /// The record's generation number.
        generation: u64,
        /// The SHA-256 of the record's exact bytes, which identifies the
        /// generation exactly.
        record_sha256: RecordDigest,
        /// Whether it was read from the published files or from this
        /// replica's persisted last-known-good copy.
        origin: GenerationOrigin,
        /// The data-sources document.
        data_sources: Vec<DataSource>,
        /// The tenants document.
        tenants: Vec<TenantRuntimeBinding>,
        /// The catalogue, decoded by the host.
        catalogue: DecodedCatalogue,
    },
    /// No publication record exists: the documents were written by a
    /// publisher older than the record, or by hand. Nothing proves they
    /// belong together, so the view they make reports no generation. Every
    /// one was read and decoded in this pass; one that could not be is an
    /// `Err` from the source, which holds the whole previous view.
    Unrecorded {
        /// The data-sources document.
        data_sources: Vec<DataSource>,
        /// The tenants document.
        tenants: Vec<TenantRuntimeBinding>,
        /// The catalogue, decoded by the host.
        catalogue: DecodedCatalogue,
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

/// Supplies the published tenants, data sources and catalogue together.
///
/// The same rules as [`ResourceSource`](crate::ResourceSource): read only
/// what reconciliation already produced, and never turn a failure into an
/// empty set. An `Err` is a generation that cannot be applied at all --
/// incomplete, refused by its floor, or unreadable -- and leaves the whole
/// installed view as it was.
#[async_trait]
pub trait PublishedStateSource: Send + Sync {
    /// Reads all three documents once.
    ///
    /// # Errors
    ///
    /// [`SourceError`] when no generation can be applied.
    async fn load(&self) -> Result<PublishedState, SourceError>;

    /// [`Self::load`] at process start, when a source may choose
    /// differently, such as from a last-known-good copy.
    ///
    /// # Errors
    ///
    /// As [`Self::load`].
    async fn load_at_start(&self) -> Result<PublishedState, SourceError> {
        self.load().await
    }

    /// Told once the generation a load returned has been installed whole,
    /// so a source raises its floor or persists it only for a generation
    /// requests actually read. Never called for a refused candidate.
    async fn installed(&self, _generation: u64, _record_sha256: &RecordDigest) {}

    /// A short description for logging.
    fn describe(&self) -> String;

    /// What this source's last-known-good copy holds, for readiness.
    /// [`LastKnownGoodState::Disabled`] unless the source persists one.
    fn last_known_good(&self) -> LastKnownGoodState {
        LastKnownGoodState::Disabled
    }
}
