//! Settling on one of the repositories an installation reaches.

use crate::git_integration::service::install::as_selected;
use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::logging;
use crate::Operator;

impl GitIntegrationService {
    /// Settles on one of the repositories an installation can reach.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::Refused`] if the named repository is not
    /// one the installation can actually reach — an operator choosing from a
    /// stale list must not be able to point the platform at something it
    /// cannot read.
    ///
    /// [`IntegrationError::Moved`] if the integration changed while the host
    /// was being asked what the installation reaches. This transition writes
    /// the record and the key it read before that call, so it **checks**: an
    /// operator's choice prepared before a disconnect took its turn must not
    /// put back what the disconnect forgot, and a second operator's choice
    /// prepared against a repository the first has since replaced is a choice
    /// to take again rather than one to apply blind.
    ///
    /// [`IntegrationError::Unavailable`] also stands for a transition nothing
    /// watched to the end, which is not the same as one that failed.
    pub async fn choose_repository(
        &self,
        operator: &Operator,
        owner: &str,
        name: &str,
    ) -> Result<(), IntegrationError> {
        let mut prepared = self.prepared().await?;

        let installation = prepared
            .integration
            .installation
            .as_ref()
            .ok_or(IntegrationError::NotConnected)?;

        let detail = self
            .provisioning
            .inspect_installation(&prepared.integration.app_id, &prepared.key, &installation.id)
            .await?;

        let chosen = detail
            .repositories
            .iter()
            .find(|candidate| candidate.owner == owner && candidate.name == name)
            .ok_or_else(|| {
                IntegrationError::Refused(
                    "the application does not have access to that repository".to_owned(),
                )
            })?;

        if let Some(installation) = prepared.integration.installation.as_mut() {
            installation.repository = Some(as_selected(chosen));
        }

        let (subject, owner, name) = (operator.subject().to_owned(), owner.to_owned(), name.to_owned());

        self.store_and_bind(&prepared, move || {
            logging::integration_repository_chosen(&subject, &owner, &name);
        })
        .await?;

        Ok(())
    }
}
