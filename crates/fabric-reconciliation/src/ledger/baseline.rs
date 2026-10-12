//! What the ledger knows about each declared client's callbacks.

use std::collections::{BTreeMap, BTreeSet};

use fabric_client_model::{OidcClient, OidcClientId};

use super::{CallbackDigest, WriteStage};

/// The callbacks Fabric has written for one declared client.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WrittenCallbacks {
    /// In the last declaration the provider confirmed writing.
    pub confirmed: BTreeSet<CallbackDigest>,

    /// In a declaration recorded as being written since, and never
    /// confirmed: possibly in the provider, possibly not.
    pub unconfirmed: BTreeSet<CallbackDigest>,
}

/// What the ledger holds for one client's declared clients. A declared
/// client with no entry is unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedBaseline {
    clients: BTreeMap<OidcClientId, WrittenCallbacks>,
}

impl AppliedBaseline {
    /// Knows nothing about any client: what a deployment with no ledger, or
    /// an unreadable one, reconciles against.
    #[must_use]
    pub fn unknown() -> Self {
        Self::default()
    }

    /// Replays one record, in the order the ledger stored them: a `Writing`
    /// record adds unconfirmed callbacks, and a `Written` record replaces
    /// the confirmed set and clears what was unconfirmed.
    pub fn replay(&mut self, client: OidcClientId, callbacks: BTreeSet<CallbackDigest>, stage: WriteStage) {
        let written = self.clients.entry(client).or_default();
        match stage {
            WriteStage::Writing => written.unconfirmed.extend(callbacks),
            WriteStage::Written => {
                written.confirmed = callbacks;
                written.unconfirmed.clear();
            }
        }
    }

    /// What is known about `client`, if anything.
    #[must_use]
    pub fn callbacks(&self, client: &OidcClientId) -> Option<&WrittenCallbacks> {
        self.clients.get(client)
    }

    /// Whether writing `declared` must be recorded: always, unless the
    /// confirmed set is exactly its callbacks and nothing is unconfirmed.
    #[must_use]
    pub fn must_record(&self, declared: &OidcClient) -> bool {
        self.callbacks(&declared.id).is_none_or(|written| {
            !written.unconfirmed.is_empty() || written.confirmed != callback_digests(declared)
        })
    }
}

/// The digest of every callback `declared` names.
#[must_use]
pub fn callback_digests(declared: &OidcClient) -> BTreeSet<CallbackDigest> {
    declared.redirect.uris().iter().map(CallbackDigest::of).collect()
}
