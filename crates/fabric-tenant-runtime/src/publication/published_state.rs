//! What one read of the published documents yields.

use async_trait::async_trait;

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

    /// A short description for logging.
    fn describe(&self) -> String;
}
