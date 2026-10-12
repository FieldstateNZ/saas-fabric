//! A reader's last-known-good copy, kept current as it reads generations.

mod accept;

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use super::atomic_replace::clear_stale_staging;
use super::choice::{self, Choice};
use super::{LastKnownGoodState, LastKnownGoodStore};
use crate::logging;
use crate::publication::complete_generation::CompleteGeneration;
use crate::publication::parsed_generation::ParsedGeneration;
use crate::publication::CatalogueCheck;
use crate::SourceError;

/// The store, and what this process knows about it.
///
/// One mutex over plain data, never held across an `await`: reads and
/// writes of the copy happen outside it, and only the outcome is recorded.
pub(crate) struct LastKnownGood {
    store: LastKnownGoodStore,
    catalogue: Arc<dyn CatalogueCheck>,
    inner: Mutex<Inner>,
}

struct Inner {
    state: LastKnownGoodState,
    /// The highest generation served or validated on disk.
    floor: Option<u64>,
    /// The record digest of the copy on disk, so an unchanged generation is
    /// not rewritten on every refresh.
    stored: Option<String>,
}

impl LastKnownGood {
    pub(crate) fn new(store: LastKnownGoodStore, catalogue: Arc<dyn CatalogueCheck>) -> Self {
        Self {
            store,
            catalogue,
            inner: Mutex::new(Inner {
                state: LastKnownGoodState::Absent,
                floor: None,
                stored: None,
            }),
        }
    }

    pub(crate) fn state(&self) -> LastKnownGoodState {
        self.lock().state.clone()
    }

    /// Chooses the generation to prime from, persisting it if published.
    /// The published candidate is validated whole before it is compared
    /// with the copy, so one that cannot be served never displaces it.
    pub(crate) async fn at_start(
        &self,
        published: Result<Option<CompleteGeneration>, SourceError>,
        (origin, files): (&str, &[&Path]),
    ) -> Choice {
        let path = self.store.path().display().to_string();
        clear_stale_staging(self.store.path()).await;
        let copy = match self.store.load(self.catalogue.as_ref()).await {
            Ok(copy) => copy,
            Err(refusal) => {
                logging::last_known_good_refused(&path, &refusal);
                self.lock().state = LastKnownGoodState::Refused {
                    reason: refusal.to_string(),
                };
                None
            }
        };
        if let Some(copy) = &copy {
            let mut inner = self.lock();
            inner.state = LastKnownGoodState::Stored {
                generation: copy.generation(),
            };
            inner.floor = Some(copy.generation());
            inner.stored = Some(copy.complete.record_sha256());
        }
        let stored_generation = copy.as_ref().map(ParsedGeneration::generation);
        let choice = choice::at_start(self.validated(published, origin), copy);
        match &choice {
            Choice::Published(generation) => {
                if let Some(stored) = stored_generation.filter(|stored| *stored < generation.generation()) {
                    logging::last_known_good_superseded(&path, stored, generation.generation());
                }
                self.accept(generation, files).await;
            }
            Choice::LastKnownGood {
                generation,
                held_because,
            } => logging::last_known_good_served(&path, generation.generation(), held_because),
            Choice::Unrecorded | Choice::Refused(_) => {}
        }
        choice
    }

    /// Chooses whether a refresh applies what it read, persisting it if so.
    pub(crate) async fn on_refresh(
        &self,
        published: Result<Option<CompleteGeneration>, SourceError>,
        (origin, files): (&str, &[&Path]),
    ) -> Choice {
        let floor = self.lock().floor;
        let choice = choice::on_refresh(self.validated(published, origin), floor, origin);
        if let Choice::Published(generation) = &choice {
            self.accept(generation, files).await;
        }
        choice
    }

    /// A published read, validated whole: decoded, every resource valid,
    /// every binding resolvable, and the catalogue the host's when read.
    fn validated(
        &self,
        read: Result<Option<CompleteGeneration>, SourceError>,
        origin: &str,
    ) -> Result<Option<ParsedGeneration>, SourceError> {
        read?
            .map(|generation| ParsedGeneration::validate(generation, self.catalogue.as_ref(), origin))
            .transpose()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
