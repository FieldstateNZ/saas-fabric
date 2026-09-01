//! Which platform repository is live, and swapping it.

#[cfg(test)]
#[path = "binding/binding_tests.rs"]
mod binding_tests;

use std::sync::{Arc, RwLock};

use self::bound::Bound;
use crate::{ComponentDesired, DesiredState, DesiredStateError, ReleaseUnit, SafeDiagnostic};

mod bound;

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
///
/// That distinction has a third case, and it is the one most easily got wrong.
/// An operator who connected a repository last week, whose application key can
/// no longer be read, must not be told "not connected" — they would go and
/// connect it again instead of finding out why it stopped working. See
/// [`unusable`](Self::unusable).
pub struct PlatformDesiredState {
    /// What is bound, behind a lock held only long enough to clone.
    current: RwLock<Bound>,
}

impl PlatformDesiredState {
    /// A binding with no repository behind it.
    #[must_use]
    pub fn unconnected() -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(Bound::Nothing),
        })
    }

    /// Points the platform at a repository, replacing whatever was there.
    pub fn connect(&self, repository: Arc<dyn DesiredState>) {
        self.set(Bound::Repository(repository));
    }

    /// Records that a connected integration could not be made to work.
    ///
    /// The text is sanitised here rather than by the caller, because here is
    /// where it becomes something an operator reads. What arrives is whatever
    /// the composition root observed trying to build a client; what leaves is
    /// a [`SafeDiagnostic`].
    pub fn unusable(&self, detail: &str) {
        self.set(Bound::Unusable(SafeDiagnostic::sanitise(detail)));
    }

    /// Forgets the current repository.
    ///
    /// Used when an operator disconnects the integration, or when what was
    /// stored turns out to be unusable. The platform goes back to reporting
    /// itself unconnected rather than failing against something it can no
    /// longer reach.
    pub fn disconnect(&self) {
        self.set(Bound::Nothing);
    }

    /// Whether a repository is live.
    ///
    /// `false` for a connected integration that could not be built: nothing can
    /// be read through it. What separates the two is the *error* callers get,
    /// not this.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.required().is_ok()
    }

    /// The live repository, or the refusal that says why there is none.
    fn required(&self) -> Result<Arc<dyn DesiredState>, DesiredStateError> {
        match &*self.current.read().map_err(|_| DesiredStateError::Unavailable {
            detail: "the platform binding is poisoned".to_owned(),
        })? {
            Bound::Nothing => Err(DesiredStateError::NotConnected),
            Bound::Repository(repository) => Ok(Arc::clone(repository)),
            Bound::Unusable(detail) => Err(DesiredStateError::Unavailable {
                detail: detail.as_str().to_owned(),
            }),
        }
    }

    /// Replaces what is bound.
    fn set(&self, bound: Bound) {
        if let Ok(mut current) = self.current.write() {
            *current = bound;
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
