//! Removing a repository from a registry: it is read anonymously from then
//! on, because the credential is no longer presented for it.

use std::sync::Arc;

use fabric_component::Repository;

use crate::audit::RegistryOperation;
use crate::registries::service::{Inner, Listed, Live, RegistryService};
use crate::registries::{RegistryFailure, RegistryHost};
use crate::Operator;

impl RegistryService {
    /// Removes `repository` from the registry for `host`: it is read
    /// anonymously from then on.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::NotFound`] or
    /// [`RegistryFailure::RepositoryNotRegistered`], or a store failure.
    pub(crate) async fn remove_repository(
        &self,
        operator: &Operator,
        host: RegistryHost,
        repository: Repository,
    ) -> Result<Listed, RegistryFailure> {
        self.change(
            operator,
            Some(host.clone()),
            RegistryOperation::RemoveRepository,
            move |inner| remove(inner, host, repository),
        )
        .await
    }
}

/// Records the repository gone, then installs a client that no longer
/// presents the credential for it — carrying the credential's refusal mark,
/// so removing a repository never re-arms a credential its realm refused.
async fn remove(
    inner: Arc<Inner>,
    host: RegistryHost,
    repository: Repository,
) -> Result<Listed, RegistryFailure> {
    let mut records = inner.store.load().await?;
    let record = Inner::find(&mut records, &host)?;
    if !record.holds(&repository) {
        return Err(RegistryFailure::RepositoryNotRegistered {
            repository: repository.to_string(),
        });
    }
    inner.check_deployment(&host, &record.endpoint)?;
    let credential = inner.credential_of(record).await?;
    record
        .repositories
        .retain(|registered| registered.repository != repository);

    let client = inner.connect(record.connection(credential, record.repository_names()))?;
    let saved = record.clone();
    inner.store.save(&records).await?;

    inner.swap(
        &host,
        Some(Live {
            client,
            credential_unreadable: false,
        }),
    );
    Ok(inner.listed(saved))
}
