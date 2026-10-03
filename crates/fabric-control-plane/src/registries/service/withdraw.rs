//! Removing a registry's credential.
//!
//! # Why removing one is not proven
//!
//! Nothing is presented once it is gone, so there is nothing to prove; and a
//! removal that had to prove anonymous reading first could never withdraw
//! the credential from a private repository.

use std::sync::Arc;

use crate::audit::RegistryOperation;
use crate::registries::service::{Inner, Listed, Live, RegistryService};
use crate::registries::{RegistryFailure, RegistryHost};
use crate::Operator;

impl RegistryService {
    /// Removes a registry's credential: every repository under it is read
    /// anonymously from then on. Removing none is not an error.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::NotFound`], or a store failure.
    pub(crate) async fn remove_credential(
        &self,
        operator: &Operator,
        host: RegistryHost,
    ) -> Result<Listed, RegistryFailure> {
        self.change(
            operator,
            Some(host.clone()),
            RegistryOperation::RemoveCredential,
            move |inner| remove(inner, host),
        )
        .await
    }
}

/// Swaps in an anonymous client, records that none is held — putting the
/// client back if that fails — then deletes the credential.
///
/// # Why the record before the secret
///
/// A record naming a credential that was deleted is the worse half-state: it
/// reads as held and readable until the next restart finds it gone, and every
/// change to the registry then stops on it. So the record moves first; if the
/// deletion after it fails, what is left is an unreferenced token, logged.
///
/// A registry for the deployment's host recorded at an endpoint that is no
/// longer the deployment's is not read through at all, and is not read
/// through after either: its credential was never for this endpoint.
async fn remove(inner: Arc<Inner>, host: RegistryHost) -> Result<Listed, RegistryFailure> {
    let mut records = inner.store.load().await?;
    let record = Inner::find(&mut records, &host)?;
    if record.credential.is_none() {
        let unchanged = record.clone();
        return Ok(inner.listed(unchanged));
    }

    let live = match inner.check_deployment(&host, &record.endpoint) {
        Ok(()) => Some(Live {
            client: inner.connect(record.connection(None, record.repository_names()))?,
            credential_unreadable: false,
        }),
        Err(_) => None,
    };
    let previous = inner.swap(&host, live);

    record.credential = None;
    let saved = record.clone();
    if let Err(error) = inner.store.save(&records).await {
        inner.swap(&host, previous);
        return Err(error.into());
    }
    inner.forget(&host, &saved.secret_id).await;
    Ok(inner.listed(saved))
}
