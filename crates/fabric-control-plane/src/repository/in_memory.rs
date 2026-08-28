//! A desired-state repository held in memory.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

use crate::repository::{RepositoryError, StoredClient};

/// A repository backed by a map, for development and tests.
///
/// # It implements the concurrency rule, not a shortcut past it
///
/// Every write checks the expected revision and refuses a stale one, exactly
/// as the Git-backed implementation does, and every accepted write moves the
/// revision. A fake that returned the same revision forever, or accepted any
/// revision, would make every test of the control plane's conflict handling
/// pass regardless of whether the handling existed (specification §22).
///
/// What it does **not** do is claim to be durable. Restarting loses everything,
/// which is why the host only offers it as a development adapter and says so
/// at startup.
#[derive(Default)]
pub struct InMemoryClientRepository {
    /// The stored clients, keyed by id.
    pub(super) clients: Mutex<BTreeMap<ClientId, StoredClient>>,

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
    /// Returns [`RepositoryError`] only if the generated revision could not be
    /// parsed, which `rev-<digits>` never fails at.
    pub fn insert(&self, document: ClientDocument) -> Result<ClientRevision, RepositoryError> {
        let revision = self.next_revision()?;
        let client = document.client().id.clone();

        lock(&self.clients).insert(
            client,
            StoredClient {
                document,
                revision: revision.clone(),
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
