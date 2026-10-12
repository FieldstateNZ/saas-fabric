//! The published documents as mounted files, read as one generation.
//!
//! The read itself is in `generation_read`, and turning what the read and
//! the last-known-good copy decided into a state in `apply_choice`.

mod apply_choice;
mod generation_read;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use super::last_known_good::LastKnownGood;
use super::record::PUBLICATION_RECORD_FILE;
use super::{CatalogueCheck, LastKnownGoodState, LastKnownGoodStore, PublishedState, PublishedStateSource};
use crate::SourceError;
use apply_choice::to_choice;
use generation_read::Catalogue;

/// The tenants and data-sources files, and the publication record beside the
/// tenants file.
///
/// The record is found beside `tenants.json` rather than configured: the
/// publisher writes it there, and in Kubernetes it is a key of the tenants
/// `ConfigMap`, so any mount that shows the tenants payload shows the record
/// with it, swapped in the same kubelet update.
pub struct PublishedFiles {
    tenants: PathBuf,
    data_sources: PathBuf,
    record: PathBuf,
    catalog: Option<PathBuf>,
    last_known_good: Option<LastKnownGood>,
}

impl PublishedFiles {
    /// Reads the two documents at the runtime's configured paths.
    #[must_use]
    pub fn new(tenants_path: impl AsRef<Path>, data_sources_path: impl AsRef<Path>) -> Self {
        let tenants = tenants_path.as_ref().to_path_buf();
        Self {
            record: tenants.with_file_name(PUBLICATION_RECORD_FILE),
            tenants,
            data_sources: data_sources_path.as_ref().to_path_buf(),
            catalog: None,
            last_known_good: None,
        }
    }

    /// Also reads the catalogue at `catalog_path`, at start only:
    /// [`PublishedStateSource::load_at_start`]. A running replica keeps the
    /// catalogue it started with.
    #[must_use]
    pub fn with_catalogue(self, catalog_path: impl AsRef<Path>) -> Self {
        Self {
            catalog: Some(catalog_path.as_ref().to_path_buf()),
            ..self
        }
    }

    /// Persists every complete generation read to `store`, and primes from
    /// it when no published generation at or above it can be applied: see
    /// the `last_known_good` module for the rules. A generation is persisted
    /// only with its catalogue, so this needs [`Self::with_catalogue`].
    /// `catalogue` is the host's own catalogue decoder: no generation is
    /// chosen, kept or persisted unless the host could serve its catalogue.
    #[must_use]
    pub fn with_last_known_good(self, store: LastKnownGoodStore, catalogue: Arc<dyn CatalogueCheck>) -> Self {
        Self {
            last_known_good: Some(LastKnownGood::new(store, catalogue)),
            ..self
        }
    }

    /// Every file the publisher writes that this reader reads, which the
    /// last-known-good directory must stay apart from.
    fn published(&self) -> Vec<&Path> {
        [&self.tenants, &self.data_sources, &self.record]
            .into_iter()
            .chain(self.catalog.as_ref())
            .map(PathBuf::as_path)
            .collect()
    }
}

#[async_trait]
impl PublishedStateSource for PublishedFiles {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let choice = match (&self.last_known_good, &self.catalog) {
            (Some(held), Some(path)) => {
                let read = self.read(Catalogue::IfMatching(path)).await;
                held.on_refresh(read, (&self.describe(), &self.published())).await
            }
            (Some(held), None) => {
                let read = self.read(Catalogue::Skip).await;
                held.on_refresh(read, (&self.describe(), &self.published())).await
            }
            (None, _) => to_choice(self.read(Catalogue::Skip).await, &self.describe()),
        };
        Ok(self.apply(choice, false).await?.0)
    }

    async fn load_at_start(&self) -> Result<(PublishedState, Option<Vec<u8>>), SourceError> {
        let catalogue = self
            .catalog
            .as_deref()
            .map_or(Catalogue::Skip, Catalogue::Require);
        let read = self.read(catalogue).await;
        let choice = match &self.last_known_good {
            Some(held) => held.at_start(read, (&self.describe(), &self.published())).await,
            None => to_choice(read, &self.describe()),
        };
        self.apply(choice, true).await
    }

    fn describe(&self) -> String {
        format!(
            "published files: tenants={}, data_sources={}, record={}",
            self.tenants.display(),
            self.data_sources.display(),
            self.record.display()
        )
    }

    fn last_known_good(&self) -> LastKnownGoodState {
        self.last_known_good
            .as_ref()
            .map_or(LastKnownGoodState::Disabled, LastKnownGood::state)
    }
}
