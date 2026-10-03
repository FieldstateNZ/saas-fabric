//! A registered repository's version tags, for the picker.

use fabric_component::Repository;

use crate::registries::service::{Inner, RegistryService};
use crate::registries::versions::{sorted, VersionTags};
use crate::registries::{RegistryFailure, RegistryHost};

impl RegistryService {
    /// The version tags `repository` has published, newest first, read
    /// through the registry it is registered under.
    ///
    /// A read, so it takes no turn and is not audited. Nothing is
    /// remembered: every call asks the registry again.
    ///
    /// # Errors
    ///
    /// [`RegistryFailure::NotFound`] or
    /// [`RegistryFailure::RepositoryNotRegistered`];
    /// [`RegistryFailure::EndpointDiffers`] for the deployment's host
    /// recorded at another endpoint; the registry's failure, with a `401`,
    /// `403` or `404` read as [`RegistryFailure::RepositoryNotReadable`].
    pub(crate) async fn versions(
        &self,
        host: &RegistryHost,
        repository: &Repository,
    ) -> Result<VersionTags, RegistryFailure> {
        let mut records = self.inner.store.load().await?;
        let record = Inner::find(&mut records, host)?;
        if !record.holds(repository) {
            return Err(RegistryFailure::RepositoryNotRegistered {
                repository: repository.to_string(),
            });
        }

        // A registry recorded for the deployment's host at an endpoint that is
        // no longer the deployment's is never read through, and no wait
        // changes that; any other that is not was not restored yet.
        self.inner.check_deployment(host, &record.endpoint)?;
        let client = self
            .inner
            .live()
            .get(host)
            .map(|live| live.client.clone())
            .ok_or_else(|| {
                RegistryFailure::Unavailable(format!(
                    "{host} is recorded and not being read through yet; see the control plane's log"
                ))
            })?;
        let answer = client
            .version_tags(repository)
            .await
            .map_err(RegistryFailure::proving_repository)?;
        Ok(sorted(RegistryFailure::readable(answer, repository.as_str())?))
    }
}
