//! Whether each stored credential's realm refused it.
//!
//! # One mark per stored credential, not per client
//!
//! A registry's client is rebuilt by changes that keep its credential —
//! adding a repository proves through a new client that then becomes the live
//! one; removing a repository builds one that presents it for one fewer. If
//! each client held its own mark, the first would leave a refusal on a client
//! that was never installed and the second would clear one no operator
//! answered, and either way the next sweep would present a credential its
//! realm already refused (ADR 0026 section 5). So the mark is kept here, by
//! the id that names the credential in the secret partition, and every client
//! built from that credential shares it. A new credential is written under a
//! new id and starts unmarked; a restart starts every one unmarked, and each
//! is presented once more.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard, PoisonError};

use crate::registries::record::SecretId;
use crate::registries::service::Inner;
use crate::registries::RegistryCredential;

impl Inner {
    /// Every mark held.
    fn marks(&self) -> MutexGuard<'_, BTreeMap<SecretId, Arc<AtomicBool>>> {
        self.marks.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `credential`, carrying the mark of the stored credential `id` names —
    /// a fresh one the first time it is asked for.
    pub(super) fn marked(&self, id: &SecretId, credential: RegistryCredential) -> RegistryCredential {
        let mark = Arc::clone(self.marks().entry(id.clone()).or_default());
        credential.marked_by(mark)
    }

    /// Holds `credential`'s mark as the one for `id`: a credential just
    /// written under a new id.
    pub(super) fn hold_mark(&self, id: &SecretId, credential: &RegistryCredential) {
        self.marks().insert(id.clone(), credential.refusal_mark());
    }

    /// Forgets the mark for `id`, whose credential no record names now.
    pub(super) fn drop_mark(&self, id: &SecretId) {
        self.marks().remove(id);
    }
}
