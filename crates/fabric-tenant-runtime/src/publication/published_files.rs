//! The published documents as mounted files, read as one generation.
//!
//! The read itself is in `generation_read`, and turning what the read and
//! the last-known-good copy decided into a state in `apply_choice`. In the
//! 121-150 line band: one source type and its trait implementation, which
//! keep the pending and installed generation between passes.

mod apply_choice;
mod generation_read;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use fabric_core::RecordDigest;

use super::last_known_good::LastKnownGood;
use super::parsed_generation::ParsedGeneration;
use super::record::PUBLICATION_RECORD_FILE;
use super::{CatalogueDecoder, LastKnownGoodState, LastKnownGoodStore, PublishedState, PublishedStateSource};
use crate::SourceError;
use apply_choice::without_copy;

/// The tenants, data-sources and catalogue files, and the publication record
/// beside the tenants file.
///
/// The record is found beside `tenants.json` rather than configured: the
/// publisher writes it there, and in Kubernetes it is a key of the tenants
/// `ConfigMap`, so any mount that shows the tenants payload shows the record
/// with it, swapped in the same kubelet update.
pub struct PublishedFiles {
    tenants: PathBuf,
    data_sources: PathBuf,
    catalog: PathBuf,
    record: PathBuf,
    decoder: Arc<dyn CatalogueDecoder>,
    last_known_good: Option<LastKnownGood>,
    progress: Mutex<Progress>,
}

/// What this reader has handed out and what was installed, kept between
/// passes.
#[derive(Default)]
struct Progress {
    /// The published generation the last load chose, until it is installed
    /// and, with a copy configured, persisted.
    pending: Option<ParsedGeneration>,
    /// The highest recorded generation installed. Once there is one, a pass
    /// that finds no record holds it rather than reading the documents on
    /// their own (G8).
    recorded: Option<u64>,
}

impl PublishedFiles {
    /// Reads the three documents at the runtime's configured paths, the
    /// catalogue through the host's `decoder`.
    #[must_use]
    pub fn new(
        tenants_path: impl AsRef<Path>,
        data_sources_path: impl AsRef<Path>,
        catalog_path: impl AsRef<Path>,
        decoder: Arc<dyn CatalogueDecoder>,
    ) -> Self {
        let tenants = tenants_path.as_ref().to_path_buf();
        Self {
            record: tenants.with_file_name(PUBLICATION_RECORD_FILE),
            tenants,
            data_sources: data_sources_path.as_ref().to_path_buf(),
            catalog: catalog_path.as_ref().to_path_buf(),
            decoder,
            last_known_good: None,
            progress: Mutex::default(),
        }
    }

    /// Persists every installed generation to `store`, and primes from it
    /// when no published generation at or above it can be applied: see the
    /// `last_known_good` module for the rules. The copy is judged with the
    /// same decoder as the published files.
    #[must_use]
    pub fn with_last_known_good(self, store: LastKnownGoodStore) -> Self {
        Self {
            last_known_good: Some(LastKnownGood::new(store, Arc::clone(&self.decoder))),
            ..self
        }
    }

    /// Every file the publisher writes that this reader reads, which the
    /// last-known-good directory must stay apart from.
    fn published(&self) -> [&Path; 4] {
        [&self.tenants, &self.data_sources, &self.catalog, &self.record].map(PathBuf::as_path)
    }

    fn progress(&self) -> std::sync::MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl PublishedStateSource for PublishedFiles {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let read = self.read().await;
        let origin = self.describe();
        let choice = match &self.last_known_good {
            Some(held) => held.on_refresh(read, &origin),
            None => without_copy(read, self.decoder.as_ref(), self.progress().recorded, &origin),
        };
        self.apply(choice).await
    }

    async fn load_at_start(&self) -> Result<PublishedState, SourceError> {
        let read = self.read().await;
        let origin = self.describe();
        let choice = match &self.last_known_good {
            Some(held) => held.at_start(read, &origin).await,
            None => without_copy(read, self.decoder.as_ref(), None, &origin),
        };
        self.apply(choice).await
    }

    async fn installed(&self, generation: u64, record_sha256: &RecordDigest) {
        let pending = {
            let mut progress = self.progress();
            progress.recorded = progress.recorded.max(Some(generation));
            progress
                .pending
                .take()
                .filter(|pending| pending.complete.record_digest() == *record_sha256)
        };
        if let (Some(held), Some(pending)) = (&self.last_known_good, pending) {
            held.accept(&pending, &self.published()).await;
        }
    }

    fn describe(&self) -> String {
        format!(
            "published files: tenants={}, data_sources={}, catalog={}, record={}",
            self.tenants.display(),
            self.data_sources.display(),
            self.catalog.display(),
            self.record.display()
        )
    }

    fn last_known_good(&self) -> LastKnownGoodState {
        self.last_known_good
            .as_ref()
            .map_or(LastKnownGoodState::Disabled, LastKnownGood::state)
    }
}
