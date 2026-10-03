//! Removing a registry.
//!
//! # What removing one does not do
//!
//! It forgets what this platform holds — the live client, the credential and
//! the record — and nothing a catalogue recorded from it. The catalogue and
//! `components.yaml` hold full repository names, so what they resolved stays
//! resolved; selecting another version needs the registry back (ADR 0026
//! section 5). Removing the deployment's host's registry restores the
//! deployment's anonymous default.

use std::sync::Arc;

use crate::audit::RegistryOperation;
use crate::registries::service::{Inner, RegistryService};
use crate::registries::{RegistryFailure, RegistryHost};
use crate::Operator;

impl RegistryService {
    /// Removes the registry for `host`.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::NotFound`], or a store failure — in which case the
    /// registry is still read through as it was.
    pub(crate) async fn remove(
        &self,
        operator: &Operator,
        host: RegistryHost,
    ) -> Result<(), RegistryFailure> {
        self.change(
            operator,
            Some(host.clone()),
            RegistryOperation::Remove,
            move |inner| remove(inner, host),
        )
        .await
    }
}

/// The client first, so nothing reads through a registry being forgotten;
/// then the record, putting the client back if that fails; then the
/// credential, whose deletion failing leaves only an unreferenced token,
/// logged — never a record naming a credential that is gone.
async fn remove(inner: Arc<Inner>, host: RegistryHost) -> Result<(), RegistryFailure> {
    let mut records = inner.store.load().await?;
    let record = Inner::find(&mut records, &host)?.clone();
    let previous = inner.swap(&host, None);

    records.retain(|kept| kept.host != host);
    if let Err(error) = inner.store.save(&records).await {
        inner.swap(&host, previous);
        return Err(error.into());
    }
    if record.credential.is_some() {
        inner.forget(&host, &record.secret_id).await;
    }
    Ok(())
}
