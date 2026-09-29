//! The clients registries are read through now, and what is listed.
//!
//! # Why a change replaces one client and keeps the rest
//!
//! A client holds its tokens and whether its realm refused its credential.
//! Rebuilding every client for one registry's change would discard another
//! registry's refusal mark, and present a credential its realm already
//! refused again — the retry into a locked account ADR 0026 section 5 rules
//! out. So a change swaps its own registry's client, and the connector is
//! handed the whole set at once.

use std::collections::BTreeMap;
use std::sync::{MutexGuard, PoisonError};

use crate::registries::service::{CredentialState, Inner, Listed, Live, RegistryService};
use crate::registries::{DeploymentRegistry, RegistryFailure, RegistryHost, RegistryRecord};

impl Inner {
    /// The live set.
    pub(super) fn live(&self) -> MutexGuard<'_, BTreeMap<RegistryHost, Live>> {
        self.live.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Puts `live` in place for `host` — or takes the host out, for `None` —
    /// installs the set, and hands back what was there.
    pub(super) fn swap(&self, host: &RegistryHost, live: Option<Live>) -> Option<Live> {
        let mut held = self.live();
        let previous = match live {
            Some(live) => held.insert(host.clone(), live),
            None => held.remove(host),
        };
        let clients = held
            .iter()
            .map(|(host, live)| (host.clone(), live.client.clone()))
            .collect();
        drop(held);
        self.connector.install(clients);
        previous
    }

    /// `record`, and what is true of it now.
    pub(super) fn listed(&self, record: RegistryRecord) -> Listed {
        let live = self.live().get(&record.host).cloned();
        let credential = match &live {
            Some(live) if live.credential_unreadable => CredentialState::Unreadable,
            Some(live) if live.client.credential_refused() => CredentialState::Refused,
            _ => CredentialState::Presented,
        };
        Listed {
            installed: live.is_some(),
            credential,
            deployment: self
                .deployment
                .as_ref()
                .is_some_and(|deployment| deployment.host == record.host.as_str()),
            record,
        }
    }
}

impl RegistryService {
    /// Every registry recorded, and what is true of each now, with the
    /// deployment's own registry.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::StoreUnavailable`] or
    /// [`RegistryFailure::StoreInvalid`] if the record set could not be read.
    pub(crate) async fn list(&self) -> Result<(Vec<Listed>, Option<&DeploymentRegistry>), RegistryFailure> {
        let records = self.inner.store.load().await?;
        let listed = records
            .into_iter()
            .map(|record| self.inner.listed(record))
            .collect();
        Ok((listed, self.inner.deployment.as_ref()))
    }
}
