//! A reader's last-known-good copy, kept current as it reads generations.

use std::io;
use std::sync::{Mutex, PoisonError};

use super::atomic_replace::{clear_stale_staging, Step};
use super::choice::{self, Choice};
use super::{LastKnownGoodState, LastKnownGoodStore};
use crate::logging;
use crate::publication::complete_generation::CompleteGeneration;
use crate::SourceError;

/// The store, and what this process knows about it.
///
/// One mutex over plain data, never held across an `await`: reads and
/// writes of the copy happen outside it, and only the outcome is recorded.
pub(crate) struct LastKnownGood {
    store: LastKnownGoodStore,
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
    pub(crate) fn new(store: LastKnownGoodStore) -> Self {
        Self {
            store,
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
    pub(crate) async fn at_start(
        &self,
        published: Result<Option<CompleteGeneration>, SourceError>,
    ) -> Choice {
        let path = self.store.path().display().to_string();
        clear_stale_staging(self.store.path()).await;
        let copy = match self.store.load().await {
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
                generation: copy.generation,
            };
            inner.floor = Some(copy.generation);
            inner.stored = Some(copy.record_sha256());
        }
        let stored_generation = copy.as_ref().map(|copy| copy.generation);
        let choice = choice::at_start(published, copy);
        match &choice {
            Choice::Published(generation) => {
                if let Some(stored) = stored_generation.filter(|stored| *stored < generation.generation) {
                    logging::last_known_good_superseded(&path, stored, generation.generation);
                }
                self.accept(generation).await;
            }
            Choice::LastKnownGood {
                generation,
                held_because,
            } => logging::last_known_good_served(&path, generation.generation, held_because),
            Choice::Unrecorded | Choice::Refused(_) => {}
        }
        choice
    }

    /// Chooses whether a refresh applies what it read, persisting it if so.
    pub(crate) async fn on_refresh(
        &self,
        published: Result<Option<CompleteGeneration>, SourceError>,
        origin: &str,
    ) -> Choice {
        let floor = self.lock().floor;
        let choice = choice::on_refresh(published, floor, origin);
        if let Choice::Published(generation) = &choice {
            self.accept(generation).await;
        }
        choice
    }

    /// Raises the floor to `generation` and persists it, unless the copy on
    /// disk already is it or its catalogue has not been read yet (a later
    /// pass persists it once the catalogue volume converges).
    async fn accept(&self, generation: &CompleteGeneration) {
        let sha256 = generation.record_sha256();
        {
            let mut inner = self.lock();
            inner.floor = inner.floor.max(Some(generation.generation));
            if inner.stored.as_deref() == Some(sha256.as_str()) || generation.catalog.is_none() {
                return;
            }
        }
        let path = self.store.path().display().to_string();
        let written = self
            .store
            .persist(generation, |_: Step| -> io::Result<()> { Ok(()) })
            .await;
        let mut inner = self.lock();
        match written {
            Ok(()) => {
                logging::last_known_good_persisted(&path, generation.generation);
                inner.state = LastKnownGoodState::Stored {
                    generation: generation.generation,
                };
                inner.stored = Some(sha256);
            }
            Err(error) => {
                logging::last_known_good_persist_failed(&path, generation.generation, &error);
                inner.state = LastKnownGoodState::PersistFailed {
                    generation: generation.generation,
                    reason: error.to_string(),
                };
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
