//! What an adapter must be able to do with desired state.

use crate::{ComponentDesired, DesiredRevision, DesiredStateError, Hold, Release, ReleaseUnit};

/// Where an environment's desired state is kept.
///
/// Implemented by an adapter that knows how the platform repository is laid
/// out. Nothing here knows which files carry a pin.
///
/// # Every operation must be bounded
///
/// A contract on the implementation, not a hope. The platform binding holds a
/// lock across these calls, so the longest one can take is the longest an
/// operator's disconnect can wait — and that is cut off by the API's request
/// timeout. Bounding each network request is not enough: an operation is many
/// requests, and a host answering every one just inside its limit runs for
/// minutes. Bound the *operation*, and answer
/// [`Unavailable`](DesiredStateError::Unavailable) when the budget is spent.
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
    /// [`Conflict`](DesiredStateError::Conflict) if the state moved since it was read.
    async fn advance(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        at: &DesiredRevision,
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
    /// [`Conflict`](DesiredStateError::Conflict) if the state moved since it was read.
    async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
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
    /// [`Conflict`](DesiredStateError::Conflict) if the state moved since it was read.
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
    /// [`Conflict`](DesiredStateError::Conflict) if the state moved since it was read.
    async fn resume(
        &self,
        environment: &str,
        component: &str,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}
