//! The port through which an environment's desired state is read and moved.

use crate::ReleaseUnit;

mod component;

pub use component::{ComponentDesired, Hold};

/// What can go wrong reading or moving desired state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DesiredStateError {
    /// Nothing is connected. No operator has connected a platform repository.
    ///
    /// # Not a failure, and not the same as one
    ///
    /// A platform nobody has connected yet is a running platform waiting for
    /// an operator, and a console can say so. A platform whose *connected*
    /// repository cannot be read is broken and needs looking at.
    ///
    /// Collapsing them would tell an operator "nothing is connected" about an
    /// integration they connected last week, and they would go and connect it
    /// again rather than find out why it stopped working.
    #[error("no platform repository is connected")]
    NotConnected,

    /// No such environment, or no such component in it.
    #[error("{what} is not something this platform describes")]
    NotFound {
        /// What was asked for.
        what: String,
    },

    /// Something the write was editing changed since it was read.
    ///
    /// Not a failure so much as an instruction: the decision was taken against
    /// state that has moved, so it has to be taken again.
    #[error("desired state changed while it was being written")]
    Conflict,

    /// The store could not be reached, or failed internally.
    #[error("desired state is unavailable: {detail}")]
    Unavailable {
        /// What was observed, with no credential in it.
        detail: String,
    },

    /// The store understood the request and refused it.
    #[error("desired state refused the change: {detail}")]
    Refused {
        /// What was observed, with no credential in it.
        detail: String,
    },
}

/// Where an environment's desired state is kept.
///
/// Implemented by an adapter that knows how the platform repository is laid
/// out. Nothing here knows which files carry a pin — that is the platform
/// repository's own statement, and asking this port to move a component is the
/// whole of what this crate does about it.
#[async_trait::async_trait]
pub trait DesiredState: Send + Sync {
    /// Every component an environment describes.
    ///
    /// Read rather than configured, so adding a component to the platform
    /// repository is enough to have it reconciled — a second list in Fabric's
    /// configuration would be a second thing to keep in step, and the failure
    /// when it drifted would be a component nothing was looking after.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError`] if the environment cannot be read.
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError>;

    /// What an environment is asked to run of a component.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError`] if it cannot be read.
    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError>;

    /// Moves a component onto a release unit.
    ///
    /// Takes the unit discovery assembled, so the version, the source commit
    /// and every image's digest travel together. There is no way to express
    /// moving one image, changing a policy, or clearing a hold.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn advance(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        message: &str,
    ) -> Result<(), DesiredStateError>;

    /// Pauses advancement, leaving the desired version exactly where it is.
    ///
    /// # Why this is a separate operation and not an argument to `advance`
    ///
    /// `advance` is what the *selector* calls, and it must remain unable to
    /// express a hold — that is what guarantees an automatic pass cannot clear
    /// one in order to succeed. These are what an *operator* calls. Two verbs
    /// with two callers beats one verb with a flag that only one caller is
    /// trusted to set.
    ///
    /// The version is untouched, so no deployment overlay changes: pausing
    /// stops the environment moving, and does not move it.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn pause(
        &self,
        environment: &str,
        component: &str,
        hold: &Hold,
        message: &str,
    ) -> Result<(), DesiredStateError>;

    /// Lifts a hold, leaving the desired version exactly where it is.
    ///
    /// Only the hold. Resuming says "you may advance again", not "advance
    /// now" — the next sweep decides that, from what it observes then rather
    /// than from what was true when the operator clicked.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn resume(
        &self,
        environment: &str,
        component: &str,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}
