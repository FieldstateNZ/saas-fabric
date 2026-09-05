//! Completing an installation, and adopting a repository when there is one.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{AccessibleRepository, FlowStep, Installation, SelectedRepository};
use crate::logging;

impl GitIntegrationService {
    /// Records an installation, having first proved a token can be minted.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotOurFlow`] if the callback does not name
    /// a live flow, [`IntegrationError::HostRefused`] if no token could be
    /// minted for the installation, or a store failure.
    ///
    /// [`IntegrationError::Moved`] if an operator disconnected while the host
    /// was being asked about the installation. This transition writes the
    /// record it read, so it **checks**: an installation callback arriving
    /// after a disconnect has taken its turn must not put the record back.
    ///
    /// [`IntegrationError::Unavailable`] also stands for a transition nothing
    /// watched to the end, which is not the same as one that failed.
    pub async fn complete_install(&self, installation_id: &str, state: &str) -> Result<(), IntegrationError> {
        let flow = self
            .flows
            .consume(state, FlowStep::Installation, self.clock.now_unix_seconds())
            .ok_or(IntegrationError::NotOurFlow)?;

        let mut prepared = self.prepared().await?;

        // The mint is the verification. Recording an installation this
        // platform cannot act as would produce a console that says connected
        // and a reconciliation loop that fails every sweep.
        let detail = self
            .provisioning
            .inspect_installation(&prepared.integration.app_id, &prepared.key, installation_id)
            .await?;

        prepared.integration.installation = Some(Installation {
            id: installation_id.to_owned(),
            account: detail.account,
            repository: settle(&detail.repositories),
        });

        // Owned, because the line is written inside the transition, which
        // outlives this request and its borrows.
        let (subject, id) = (flow.operator.clone(), installation_id.to_owned());
        let settled_on_a_repository = prepared.integration.repository().is_some();

        self.store_and_bind(&prepared, move || {
            logging::integration_installed(&subject, &id, settled_on_a_repository);
        })
        .await?;

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
///
/// Shared with `choose.rs`: adopting the only repository and settling on one
/// the operator picked produce the same value, and two copies of that would be
/// two places for the layout to drift apart.
pub(super) fn as_selected(repository: &AccessibleRepository) -> SelectedRepository {
    SelectedRepository::conventional(&repository.owner, &repository.name, &repository.default_branch)
}
