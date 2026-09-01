//! What a connected integration actually points at.

use std::sync::Arc;

use crate::git_integration::{DesiredStateFactory, GitIntegration, SecretValue};
use crate::repository::DesiredStateBinding;

/// What an integration drives once it is usable.
///
/// # Why this is a port and not two fields
///
/// The connection flow — create an application, install it, choose a
/// repository, store the record — is identical for both integrations. What
/// differs is the *last step*: client configuration binds a
/// [`ClientRepository`](crate::ClientRepository), platform management binds a
/// platform desired-state repository, and the two have nothing in common but
/// the moment they happen.
///
/// Holding a factory and a binding directly made that last step client-shaped
/// all the way down. One port makes the flow indifferent to what it is
/// connecting, which is what lets there be two of it without two copies of the
/// callback correlation, the key handling, or the record semantics.
pub trait IntegrationTarget: Send + Sync {
    /// Point whatever this integration drives at the repository it describes.
    ///
    /// # Errors
    ///
    /// Returns a message if the integration does not describe a usable
    /// repository, or a client for it cannot be built.
    fn bind(&self, integration: &GitIntegration, private_key: &SecretValue) -> Result<(), String>;

    /// Forget it.
    ///
    /// Called when an operator disconnects, and when a stored integration
    /// turns out not to name a repository yet. The second is not a failure:
    /// an application can be installed and waiting for somebody to say where
    /// to look.
    fn unbind(&self);

    /// Record that a stored integration could not be made to work.
    ///
    /// Called when a record exists — somebody connected this — and it could
    /// not be bound. The distinction matters to whoever reads the console:
    /// "nothing is connected" sends an operator to connect it again, and
    /// "connected, and failing" sends them to find out why.
    ///
    /// Does nothing by default. Client configuration reports its own health
    /// from what its sweep observes, which is a stronger signal than what was
    /// true at startup, and a target with nowhere to put this should not be
    /// made to invent somewhere.
    fn unusable(&self, _detail: &str) {}
}

/// The target that client configuration binds.
pub struct ClientConfigurationTarget {
    /// Builds a repository from an integration.
    factory: Arc<dyn DesiredStateFactory>,

    /// What the rest of the platform reads through.
    binding: Arc<DesiredStateBinding>,
}

impl ClientConfigurationTarget {
    /// Builds a target over the factory and binding it drives.
    #[must_use]
    pub fn new(factory: Arc<dyn DesiredStateFactory>, binding: Arc<DesiredStateBinding>) -> Self {
        Self { factory, binding }
    }
}

impl IntegrationTarget for ClientConfigurationTarget {
    fn bind(&self, integration: &GitIntegration, private_key: &SecretValue) -> Result<(), String> {
        let repository = self.factory.build(integration, private_key)?;
        let described = repository.describe();
        self.binding.bind(repository);
        crate::logging::integration_bound(&described);

        Ok(())
    }

    fn unbind(&self) {
        self.binding.unbind();
    }
}
