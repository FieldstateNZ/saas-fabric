//! The port through which an environment's desired state is read and moved.

use crate::{Release, ReleaseUnit};

mod component;
mod errors;

pub use component::{ComponentDesired, Hold};
pub use errors::DesiredStateError;

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

    /// Moves a component onto a release.
    ///
    /// Takes what discovery assembled, so everything that must move together
    /// does: for images, the version, the source commit and every digest; for
    /// a chart, the version, which is the whole of it. There is no way to
    /// express moving one image, changing a policy, or clearing a hold.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn advance(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        message: &str,
    ) -> Result<(), DesiredStateError>;

    /// Moves a component onto an older release unit and holds it there.
    ///
    /// # It takes a `ReleaseUnit`, and that is the point
    ///
    /// Rolling back is offered for images and not for charts, and the
    /// signature is where that is settled rather than a check somewhere.
    /// A chart repository pins a version, not a digest: the bytes behind
    /// `7.3.0` can be republished, so "put me back on what I was running" is a
    /// promise it cannot keep. Until that lifecycle is modelled there is
    /// nothing here for a chart to be passed as.
    ///
    /// # Why the version and the hold are one operation
    ///
    /// A rollback that wrote the version and then the hold could be
    /// interrupted between them, and what it would leave is an environment
    /// moved backwards with automatic advancement still live — so the next
    /// sweep would undo it, and the operator would watch their rollback
    /// disappear. One commit or neither.
    ///
    /// # It takes a unit, so there is nothing to disagree about
    ///
    /// The version, the source commit and every image digest travel together,
    /// resolved from a registry rather than supplied. There is no way to
    /// express rolling back to a version with somebody else's digests.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        hold: &Hold,
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
