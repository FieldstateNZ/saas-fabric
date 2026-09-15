//! A desired-state repository held in memory.
//!
//! In the 121–150 line band. The reason is that this is one struct
//! together with its constructor and the handful of methods every other
//! file in this module needs from it — `insert` and `set_unavailable` for
//! tests, `next_revision` and `check_available` for
//! `in_memory_behaviour.rs`'s trait impl. Splitting those off `Self` would
//! not shrink this file so much as move its methods one file over, still
//! needing the same fields.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

use crate::repository::RepositoryError;

pub(super) use super::in_memory_records::{CatalogueRecord, ClientRecord};

/// A repository backed by a map, for tests.
///
/// # It implements the concurrency rule, not a shortcut past it
///
/// Every write checks the expected revision and refuses a stale one, exactly
/// as the Git-backed implementation does, and every accepted write moves the
/// revision. A fake that returned the same revision forever, or accepted any
/// revision, would make every test of the control plane's conflict handling
/// pass regardless of whether the handling existed (specification §22).
///
/// # Nothing but tests selects it
///
/// The host's development posture (`DesiredStateConfig::LocalDirectory`)
/// opens the persistent `LocalClientRepository` in `fabric-control-plane-api`
/// instead, precisely because restarting this one loses everything — a
/// property worth keeping here, where a test *wants* a clean repository every
/// run, and not worth keeping in anything a developer runs more than once.
/// This type is `pub` only so integration tests outside this crate (a
/// separate compilation unit, with no access to anything `pub(crate)`) can
/// build a router against it.
#[derive(Default)]
pub struct InMemoryClientRepository {
    /// The stored clients, keyed by id, each rendered — never the typed
    /// struct. The same reasoning as `catalogue`, applied to the other kind
    /// of document this repository holds.
    pub(super) clients: Mutex<BTreeMap<ClientId, ClientRecord>>,

    /// The catalogue, rendered — never the typed struct.
    ///
    /// Storing text and parsing it back on every read is not the obvious
    /// choice for an in-memory fake, but it is the one that keeps this
    /// repository honest: the Git-backed and local stores both go through
    /// `Catalogue::render`/`Catalogue::parse` on every write and read, which
    /// is where an envelope, validation or size bug would actually be
    /// caught. Holding the typed struct directly would make this the one
    /// repository an HTTP test could drive without ever exercising that
    /// round trip.
    pub(super) catalogue: Mutex<Option<CatalogueRecord>>,

    /// The number of writes so far, which is where revisions come from.
    writes: Mutex<u64>,

    /// An injected failure, for tests that need an unreachable repository.
    unavailable: Mutex<Option<String>>,
}

impl InMemoryClientRepository {
    /// Builds an empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a client, as if it had always been there.
    ///
    /// Returns the revision it was stored at, so a test can write against it.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if the generated revision could not be
    /// parsed (which `rev-<digits>` never fails at), or if `document` will
    /// not render — the same round trip [`ClientRepository::create`](crate::ClientRepository::create)
    /// applies to every client this repository is asked to store.
    pub fn insert(&self, document: &ClientDocument) -> Result<ClientRevision, RepositoryError> {
        let text = document.render().map_err(|_| RepositoryError::Rejected {
            detail: "Invalid client document".into(),
        })?;
        let revision = self.next_revision()?;
        let client = document.client().id.clone();

        lock(&self.clients).insert(
            client,
            ClientRecord {
                revision: revision.clone(),
                text,
            },
        );

        Ok(revision)
    }

    /// Makes every subsequent operation report the repository as unavailable.
    pub fn set_unavailable(&self, detail: Option<String>) {
        *lock(&self.unavailable) = detail;
    }

    /// The revision an accepted write moves to.
    ///
    /// A counter rather than a content hash, because a revision is opaque and
    /// compared only for equality — see
    /// [`ClientRevision`](fabric_client_model::ClientRevision). A counter also
    /// makes a *no-change* rewrite produce a new revision, which is the
    /// pessimistic behaviour and therefore the one worth testing against.
    ///
    /// The parse cannot fail for `rev-<digits>`, and the failure is returned
    /// rather than unwrapped anyway: the workspace denies `unwrap`, and an
    /// error is a better answer than a plausible-looking placeholder that two
    /// writes could share.
    pub(super) fn next_revision(&self) -> Result<ClientRevision, RepositoryError> {
        let mut writes = lock(&self.writes);
        *writes += 1;

        ClientRevision::try_new(format!("rev-{writes}")).map_err(|error| RepositoryError::Unavailable {
            detail: error.to_string(),
        })
    }

    /// Reports the injected failure, if one is set.
    pub(super) fn check_available(&self) -> Result<(), RepositoryError> {
        match lock(&self.unavailable).clone() {
            None => Ok(()),
            Some(detail) => Err(RepositoryError::Unavailable { detail }),
        }
    }
}

/// Takes a lock, recovering from a poisoned one rather than panicking.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
