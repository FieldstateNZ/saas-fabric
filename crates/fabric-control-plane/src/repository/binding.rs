//! Which desired-state repository is live, and swapping it.
//!
//! The same device the operator posture uses for its signing keys, for the
//! same reason: something outside decides what the current value is, and
//! everything on the request path reads it without blocking on that decision.

use std::sync::{Arc, RwLock};

use crate::repository::{ClientRepository, UnconfiguredRepository};

/// The desired-state repository this control plane is currently bound to.
///
/// Starts unbound. A control plane that refused to start without a repository
/// could not be used to connect one, which is the whole point of the operator
/// console — so an unbound platform is a running platform that says so.
pub struct DesiredStateBinding {
    /// The live repository, behind a lock held only long enough to clone.
    ///
    /// `None` is the unbound state. The `Option` lives *here* and nowhere
    /// else: [`current`](Self::current) hands out a repository either way, so
    /// no caller has to decide what absence means, while
    /// [`is_configured`](Self::is_configured) can still answer truthfully for
    /// the one caller whose whole job is to report it.
    current: RwLock<Option<Arc<dyn ClientRepository>>>,
}

impl DesiredStateBinding {
    /// A binding with no repository behind it.
    #[must_use]
    pub fn unconfigured() -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(None),
        })
    }

    /// A binding already pointed at a repository.
    ///
    /// For the deployment that states where desired state lives in its own
    /// configuration, and for tests.
    #[must_use]
    pub fn to(repository: Arc<dyn ClientRepository>) -> Arc<Self> {
        Arc::new(Self {
            current: RwLock::new(Some(repository)),
        })
    }

    /// Points the platform at a repository, replacing whatever was there.
    pub fn bind(&self, repository: Arc<dyn ClientRepository>) {
        self.set(Some(repository));
    }

    /// Forgets the current repository.
    ///
    /// Used when an integration is disconnected or found to be invalid. The
    /// platform goes back to reporting itself unconfigured rather than
    /// continuing to fail against a repository it can no longer reach.
    pub fn unbind(&self) {
        self.set(None);
    }

    /// Whether this platform has been connected to a repository at all.
    ///
    /// The one question [`current`](Self::current) cannot answer, and the
    /// reason the `Option` above is not simply collapsed away: "nobody has
    /// connected this yet" and "the repository is refusing us" lead an
    /// operator somewhere completely different.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.read().is_some()
    }

    /// The repository to use for this operation.
    #[must_use]
    pub fn current(&self) -> Arc<dyn ClientRepository> {
        self.read().unwrap_or_else(|| Arc::new(UnconfiguredRepository))
    }

    /// Stores a new binding.
    ///
    /// A poisoned lock means a thread panicked while holding it. This one only
    /// ever swaps an `Option<Arc<_>>`, so there is no torn state to protect,
    /// and leaving the platform permanently unable to rebind would be the
    /// worse outcome.
    fn set(&self, repository: Option<Arc<dyn ClientRepository>>) {
        match self.current.write() {
            Ok(mut held) => *held = repository,
            Err(poisoned) => *poisoned.into_inner() = repository,
        }
    }

    /// The binding as it stands.
    fn read(&self) -> Option<Arc<dyn ClientRepository>> {
        match self.current.read() {
            Ok(held) => held.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}
