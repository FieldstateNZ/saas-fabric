//! Registering a repository under a registry.
//!
//! # Registered, not browsed
//!
//! Neither GHCR nor Docker Hub serves the catalog endpoint to an anonymous
//! client, so a registry's repositories are the ones an operator registered —
//! each proven before it is recorded, by its tag listing answering through
//! this registry, with the credential when one is held. Registering one that
//! is already registered proves it again.

use std::sync::Arc;

use fabric_component::Repository;

use crate::audit::RegistryOperation;
use crate::registries::service::{Inner, Listed, Live, RegistryService};
use crate::registries::{RegisteredRepository, RegistryFailure, RegistryHost};
use crate::Operator;

impl RegistryService {
    /// Registers `repository` under the registry for `host`, once its tag
    /// listing answers.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::Invalid`] if the repository is on another host,
    /// [`RegistryFailure::NotFound`], or the proof's failure —
    /// [`RegistryFailure::RepositoryNotReadable`] for a `401`, `403` or
    /// `404`, [`RegistryFailure::Refused`] if the realm refused the
    /// credential, which marks it.
    pub(crate) async fn add_repository(
        &self,
        operator: &Operator,
        host: RegistryHost,
        repository: Repository,
    ) -> Result<Listed, RegistryFailure> {
        let operation = RegistryOperation::AddRepository;
        if repository.host() != host.as_str() {
            let failure = RegistryFailure::Invalid(format!(
                "{repository} is not on {host}: a repository is registered under the registry its host names"
            ));
            return Self::refused(operator, Some(&host), operation, failure);
        }

        self.change(operator, Some(host.clone()), operation, move |inner| {
            add(inner, host, repository)
        })
        .await
    }
}

/// Proves the repository through a client that presents the credential for
/// it, records it, and installs that client.
async fn add(
    inner: Arc<Inner>,
    host: RegistryHost,
    repository: Repository,
) -> Result<Listed, RegistryFailure> {
    let mut records = inner.store.load().await?;
    let record = Inner::find(&mut records, &host)?;
    inner.check_deployment(&host, &record.endpoint)?;
    let credential = inner.credential_of(record).await?;
    let mut repositories = record.repository_names();
    if !record.holds(&repository) {
        repositories.push(repository.clone());
    }

    // Built with the stored credential's mark, so a realm refusing it here
    // marks the client discovery reads through as well.
    let client = inner.connect(record.connection(credential, repositories))?;
    let answer = client
        .prove_repository(&repository)
        .await
        .map_err(RegistryFailure::proving_repository)?;
    RegistryFailure::readable(answer, repository.as_str())?;

    let now = inner.clock.now_unix_seconds();
    match record
        .repositories
        .iter_mut()
        .find(|registered| registered.repository == repository)
    {
        Some(registered) => registered.proven_at = now,
        None => record.repositories.push(RegisteredRepository {
            repository,
            proven_at: now,
        }),
    }
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
