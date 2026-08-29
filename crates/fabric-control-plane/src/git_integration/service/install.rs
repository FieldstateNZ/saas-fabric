//! Completing an installation, and settling on a repository.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{AccessibleRepository, FlowStep, Installation, SelectedRepository};
use crate::logging;
use crate::Operator;

impl GitIntegrationService {
    /// Records an installation, having first proved a token can be minted.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotOurFlow`] if the callback does not name
    /// a live flow, [`IntegrationError::HostRefused`] if no token could be
    /// minted for the installation, or a store failure.
    pub async fn complete_install(&self, installation_id: &str, state: &str) -> Result<(), IntegrationError> {
        let flow = self
            .flows
            .consume(state, FlowStep::Installation, self.clock.now_unix_seconds())
            .ok_or(IntegrationError::NotOurFlow)?;

        let mut integration = self.current().await?.ok_or(IntegrationError::NotConnected)?;
        let key = self.private_key().await?;

        // The mint is the verification. Recording an installation this
        // platform cannot act as would produce a console that says connected
        // and a reconciliation loop that fails every sweep.
        let detail = self
            .provisioning
            .inspect_installation(&integration.app_id, &key, installation_id)
            .await?;

        integration.installation = Some(Installation {
            id: installation_id.to_owned(),
            account: detail.account,
            repository: settle(&detail.repositories),
        });

        self.store.save(&integration).await?;
        self.rebind(&integration, &key)
            .map_err(IntegrationError::Refused)?;

        logging::integration_installed(
            &flow.operator,
            installation_id,
            integration.repository().is_some(),
        );

        Ok(())
    }

    /// Settles on one of the repositories an installation can reach.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::Refused`] if the named repository is not
    /// one the installation can actually reach — an operator choosing from a
    /// stale list must not be able to point the platform at something it
    /// cannot read.
    pub async fn choose_repository(
        &self,
        operator: &Operator,
        owner: &str,
        name: &str,
    ) -> Result<(), IntegrationError> {
        let mut integration = self.current().await?.ok_or(IntegrationError::NotConnected)?;
        let key = self.private_key().await?;

        let installation = integration
            .installation
            .as_ref()
            .ok_or(IntegrationError::NotConnected)?;

        let detail = self
            .provisioning
            .inspect_installation(&integration.app_id, &key, &installation.id)
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

        if let Some(installation) = integration.installation.as_mut() {
            installation.repository = Some(as_selected(chosen));
        }

        self.store.save(&integration).await?;
        self.rebind(&integration, &key)
            .map_err(IntegrationError::Refused)?;

        logging::integration_repository_chosen(operator.subject(), owner, name);

        Ok(())
    }
}

/// Picks the repository when there is no choice to make.
///
/// Exactly one, and the platform adopts it — that is the ordinary case and
/// making an operator confirm it would be ceremony. More than one, and it
/// declines: guessing would write client configuration somewhere nobody
/// expects, and it would look like it worked.
fn settle(repositories: &[AccessibleRepository]) -> Option<SelectedRepository> {
    match repositories {
        [only] => Some(as_selected(only)),
        _ => None,
    }
}

/// The platform's conventional layout, applied to a repository.
fn as_selected(repository: &AccessibleRepository) -> SelectedRepository {
    SelectedRepository::conventional(&repository.owner, &repository.name, &repository.default_branch)
}
