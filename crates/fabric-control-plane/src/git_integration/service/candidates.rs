//! Asking the host what an installation can reach.
//!
//! Read fresh from the host every time rather than cached in the record. What
//! an installation can reach is the host's to change — an operator can add or
//! remove a repository from it without this platform being told, because there
//! is no webhook — so a cached list is a list that is quietly wrong.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::AccessibleRepository;

impl GitIntegrationService {
    /// Every repository the current installation can reach.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotConnected`] if nothing is installed, or
    /// a host failure.
    pub async fn accessible_repositories(&self) -> Result<Vec<AccessibleRepository>, IntegrationError> {
        let integration = self.current().await?.ok_or(IntegrationError::NotConnected)?;
        let key = self.private_key().await?;

        let installation = integration
            .installation
            .as_ref()
            .ok_or(IntegrationError::NotConnected)?;

        Ok(self
            .provisioning
            .inspect_installation(&integration.app_id, &key, &installation.id)
            .await?
            .repositories)
    }
}
