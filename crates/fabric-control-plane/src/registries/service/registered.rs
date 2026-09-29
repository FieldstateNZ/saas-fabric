//! Which repositories are registered, for a selection to be held to.

use std::collections::BTreeSet;

use fabric_component::Repository;

use crate::registries::service::RegistryService;
use crate::registries::{RegistryFailure, RegistryRecord};

impl RegistryService {
    /// Every repository registered under any registry, once `primary`'s own
    /// registry is being read through — or `None` when `primary` is not
    /// registered at all.
    ///
    /// # Why the whole set, read once
    ///
    /// A selection is refused for a primary repository that is not in it
    /// before any registry is asked, and every other repository its
    /// component descriptor names is held to it while the rule runs (ADR
    /// 0026 section 7) — so a descriptor can never make Fabric present a
    /// registry's credential to a repository an operator did not choose.
    /// The rule asks synchronously, so it is asked of this snapshot, read
    /// once, and not of the store on every image.
    ///
    /// # Why the primary's registry is checked here
    ///
    /// By the rule the version picker's listing follows
    /// ([`versions`](Self::versions)): a registry recorded for the
    /// deployment's host at another endpoint is never read through, and one
    /// recorded and not restored yet has no client, so the router would
    /// answer its host as if the registry had refused. Both are said here,
    /// before any registry is asked, as the listing says them. Only the
    /// primary's registry is known before the component descriptor is read;
    /// a sibling repository under another registry that is not being read
    /// through is answered as the router answers its host.
    ///
    /// A read: it takes no turn and is not audited.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::StoreUnavailable`] or
    /// [`RegistryFailure::StoreInvalid`] if the record set could not be read;
    /// [`RegistryFailure::EndpointDiffers`] for the deployment's host
    /// recorded at another endpoint; [`RegistryFailure::Unavailable`] for a
    /// registry recorded and not being read through yet.
    pub(crate) async fn registered_for(
        &self,
        primary: &Repository,
    ) -> Result<Option<BTreeSet<Repository>>, RegistryFailure> {
        let records = self.inner.store.load().await?;
        let Some(record) = records.iter().find(|record| record.holds(primary)) else {
            return Ok(None);
        };
        self.inner.check_deployment(&record.host, &record.endpoint)?;
        if !self.inner.live().contains_key(&record.host) {
            return Err(RegistryFailure::Unavailable(format!(
                "{} is recorded and not being read through yet; see the control plane's log",
                record.host
            )));
        }
        Ok(Some(
            records
                .iter()
                .flat_map(RegistryRecord::repository_names)
                .collect(),
        ))
    }
}
