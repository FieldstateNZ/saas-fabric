//! Taking a validated generation as the new floor, and persisting it.

use std::io;
use std::path::Path;

use super::LastKnownGood;
use crate::logging;
use crate::publication::last_known_good::atomic_replace::Step;
use crate::publication::last_known_good::{check_separation, LastKnownGoodState, LastKnownGoodStore};
use crate::publication::parsed_generation::ParsedGeneration;

impl LastKnownGood {
    /// Raises the floor to an installed `generation` and persists it, unless
    /// the copy on disk already is it, or a refused copy is still on disk:
    /// that copy is kept, bytes and refusal, until an operator discards it.
    /// Called only once the refresher has installed the generation whole.
    pub(crate) async fn accept(&self, generation: &ParsedGeneration, files: &[&Path]) {
        let sha256 = generation.complete.record_sha256();
        let refused = {
            let mut inner = self.lock();
            inner.floor = inner.floor.max(Some(generation.generation()));
            if inner.stored.as_deref() == Some(sha256.as_str()) {
                return;
            }
            matches!(inner.state, LastKnownGoodState::Refused { .. })
        };
        if refused {
            if tokio::fs::try_exists(self.store.path()).await.unwrap_or(true) {
                return;
            }
            self.lock().state = LastKnownGoodState::Absent;
        }
        let path = self.store.path().display().to_string();
        let written = match separated(&self.store, files) {
            Ok(()) => {
                self.store
                    .persist(&generation.complete, |_: Step| -> io::Result<()> { Ok(()) })
                    .await
            }
            Err(reason) => Err(io::Error::other(reason)),
        };
        let mut inner = self.lock();
        match written {
            Ok(()) => {
                logging::last_known_good_persisted(&path, generation.generation());
                inner.state = LastKnownGoodState::Stored {
                    generation: generation.generation(),
                };
                inner.stored = Some(sha256);
            }
            Err(error) => {
                logging::last_known_good_persist_failed(&path, generation.generation(), &error);
                inner.state = LastKnownGoodState::PersistFailed {
                    generation: generation.generation(),
                    reason: error.to_string(),
                };
            }
        }
    }
}

/// Proves, before every write, that the copy's directory is apart from
/// every published file `files` names.
fn separated(store: &LastKnownGoodStore, files: &[&Path]) -> Result<(), String> {
    let directory = store.path().parent().unwrap_or_else(|| Path::new("."));
    check_separation(directory, files)
}
