//! The port through which an environment's desired state is read and moved.

use crate::Release;

mod component;
mod errors;

pub use component::{ComponentDesired, DesiredRevision, Hold};
pub use errors::DesiredStateError;

/// Where an environment's desired state is kept.
///
/// Implemented by an adapter that knows how the platform repository is laid
/// out. Nothing here knows which files carry a pin.
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
    /// # `at` is the read this decision was taken against
    ///
    /// Not the read the write happens to make. A component whose policy, hold,
    /// version or artifact changed in between is a
    /// [`Conflict`](DesiredStateError::Conflict), because the decision being
    /// applied was taken about something else.
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
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError>;

    /// Moves a component onto an older release and holds it there.
    ///
    /// # It takes a whole release, and that is the point
    ///
    /// Rolling back means restoring an older published version of the
    /// component, and it is offered for either kind. What this takes is the whole release, so
    /// nothing can move on its own: for images the version, the source commit
    /// and every digest travel together, resolved from a registry rather than
    /// supplied, so rolling back to a version with somebody else's digests is
    /// not a shape that exists. For a chart the version is the whole of it —
    /// there is no digest to carry, and the repository and chart name travel
    /// with it so a release discovered against one chart cannot be written
    /// into a pin for another.
    ///
    /// The two restore different amounts, and that is stated to the operator
    /// rather than enforced here: an image rollback returns the exact bytes,
    /// a chart rollback the version, which a repository may have republished.
    ///
    /// # Why the version and the hold are one operation
    ///
    /// A rollback that wrote the version and then the hold could be
    /// interrupted between them, and what it would leave is an environment
    /// moved backwards with automatic advancement still live — so the next
    /// sweep would undo it, and the operator would watch their rollback
    /// disappear. One commit or neither.
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Conflict`] if the state moved since it was read,
    /// and the other variants otherwise.
    async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        hold: &Hold,
        at: &DesiredRevision,
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
        at: &DesiredRevision,
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
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}
