//! Reading back what this platform recorded about its integration.
//!
//! Two stores, and the pair is the point: the record says which application
//! and repository, the key is what lets the platform act as it. Every step
//! past creation needs both, and either one alone describes an integration
//! that cannot do anything.

use crate::git_integration::service::{GitIntegrationService, IntegrationError};
use crate::git_integration::{SecretName, SecretValue};
use crate::GitIntegration;

impl GitIntegrationService {
    /// The stored integration, if this platform has one.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if the store could not be read.
    pub async fn current(&self) -> Result<Option<GitIntegration>, IntegrationError> {
        self.store.load(self.kind).await.map_err(IntegrationError::from)
    }

    /// The application's private key.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError::NotConnected`] when there is no key, which
    /// is the state of a platform that has never connected — and
    /// [`IntegrationError::Unavailable`] when the store could not be read.
    pub(super) async fn private_key(&self) -> Result<SecretValue, IntegrationError> {
        self.secrets
            .get(&SecretName::new(self.kind.private_key()))
            .await?
            .ok_or(IntegrationError::NotConnected)
    }
}
