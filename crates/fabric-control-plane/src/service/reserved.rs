//! Read access to the reserved names [`ClientService::new`](crate::ClientService::new)
//! is handed, for the two write paths that check against them.

use std::collections::BTreeSet;

use crate::ClientService;

impl ClientService {
    /// Realms a new client may never declare, as case-folded strings — see
    /// [`ClientService`]'s own field for why never a `RealmName`.
    #[must_use]
    pub(crate) fn reserved_realms(&self) -> &BTreeSet<String> {
        &self.reserved_realms
    }

    /// Application ids the catalogue may never accept.
    #[must_use]
    pub(crate) fn reserved_client_ids(&self) -> &BTreeSet<String> {
        &self.reserved_client_ids
    }
}
