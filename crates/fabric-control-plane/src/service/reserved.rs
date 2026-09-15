//! Read access to the reserved names [`ClientService::new`](crate::ClientService::new)
//! is handed, for the two write paths that check against them.

use std::collections::BTreeSet;

use fabric_client_model::RealmName;

use crate::ClientService;

impl ClientService {
    /// Realms a new client may never declare.
    #[must_use]
    pub(crate) fn reserved_realms(&self) -> &BTreeSet<RealmName> {
        &self.reserved_realms
    }

    /// Application ids the catalogue may never accept.
    #[must_use]
    pub(crate) fn reserved_client_ids(&self) -> &BTreeSet<String> {
        &self.reserved_client_ids
    }
}
