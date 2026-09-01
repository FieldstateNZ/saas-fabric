//! Which platform repository is live, and swapping it.

#[cfg(test)]
#[path = "binding/binding_tests.rs"]
mod binding_tests;

use std::sync::{Arc, RwLock};

use crate::{ComponentDesired, DesiredState, DesiredStateError, ReleaseUnit};

/// The platform repository this control plane is currently connected to.
///
/// # Why this is late-bound at all
///
/// The repository and the credential are not configuration any more. An
/// operator installs a GitHub App and picks a repository, and the platform
/// stores what it learns doing so — so at startup there is nothing to build
/// from, and a control plane that refused to start without one could not be
/// used to connect one.
///
/// The same device the client desired-state binding uses, for the same reason.
///
/// # Unconnected is a state, not an error
///
/// Every operation answers [`NotConnected`](DesiredStateError::NotConnected)
/// while nothing is bound. The console renders that as "not connected"; a
/// *connected* repository that cannot be read answers something else, and an
/// operator is told which.
pub struct PlatformDesiredState {
    /// The live repository, behind a lock held only long enough to clone.
    current: RwLock<Option<Arc<dyn DesiredState>>>,
}

impl PlatformDesiredState {
    /// A binding with no repository behind it.
    #[must_use]
    pub fn unconnected() -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(None),
        })
    }

    /// Points the platform at a repository, replacing whatever was there.
    pub fn connect(&self, repository: Arc<dyn DesiredState>) {
        self.set(Some(repository));
    }

    /// Forgets the current repository.
    ///
    /// Used when an operator disconnects the integration, or when what was
    /// stored turns out to be unusable. The platform goes back to reporting
    /// itself unconnected rather than failing against something it can no
    /// longer reach.
    pub fn disconnect(&self) {
        self.set(None);
    }

    /// Whether anything is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.live().is_some()
    }

    /// The live repository, if there is one.
    fn live(&self) -> Option<Arc<dyn DesiredState>> {
        self.current.read().ok().and_then(|current| current.clone())
    }

    /// The live repository, or the refusal that says why there is none.
    fn required(&self) -> Result<Arc<dyn DesiredState>, DesiredStateError> {
        self.live().ok_or(DesiredStateError::NotConnected)
    }

    /// Replaces what is bound.
    fn set(&self, repository: Option<Arc<dyn DesiredState>>) {
        if let Ok(mut current) = self.current.write() {
            *current = repository;
        }
    }
}

#[async_trait::async_trait]
impl DesiredState for PlatformDesiredState {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        self.required()?.components(environment).await
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        self.required()?.component(environment, component).await
    }

    async fn advance(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.required()?
            .advance(environment, component, unit, message)
            .await
    }
}
