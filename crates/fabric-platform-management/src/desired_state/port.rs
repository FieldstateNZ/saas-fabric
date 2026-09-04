//! What an adapter must be able to do with desired state.

use crate::{ComponentDesired, DesiredRevision, DesiredStateError, Hold, Release, ReleaseUnit};

/// Where an environment's desired state is kept.
///
/// Implemented by an adapter that knows how the platform repository is laid
/// out. Nothing here knows which files carry a pin.
///
/// # Every operation must be bounded, and never by cancellation
///
/// A contract, not a hope: the platform binding holds a lock across these
/// calls, so the longest one can take is the longest an operator's disconnect
/// can wait, and that is cut off by the API's request timeout. Bounding each
/// request separately is not enough — an operation is many of them — so bound
/// the *operation*, answering [`Unavailable`](DesiredStateError::Unavailable)
/// when the budget is spent. Bound it by refusing to **start** a request it
/// cannot afford, never by abandoning one already sent: a write dropped
/// mid-flight releases the binding while it may still land, in a repository
/// the platform has by then reported it stopped writing to. So an operation
/// ends within its budget plus the one request it may still have running.
///
/// Every write answers [`Conflict`](DesiredStateError::Conflict) if the state
/// it was decided against has moved since it was read, and the other variants
/// for what they name — said once here rather than under each method.
#[async_trait::async_trait]
pub trait DesiredState: Send + Sync {
    /// Every component an environment describes.
    ///
    /// Read rather than configured, so a component added to the platform
    /// repository is reconciled without more; a second list in Fabric's
    /// configuration would drift, into a component nothing was looking after.
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
    /// Rolling back is offered for images and not for charts, and the signature
    /// settles that rather than a check somewhere. A chart repository pins a
    /// version, not a digest: the bytes behind `7.3.0` can be republished, so
    /// "put me back on what I was running" is a promise it cannot keep, and
    /// until that lifecycle is modelled a chart has nothing here to be passed
    /// as. And a unit travels whole — the version, the source commit and every
    /// digest, resolved from a registry rather than supplied — so there is no
    /// way to express rolling back to a version with somebody else's digests.
    ///
    /// # Why the version and the hold are one operation
    ///
    /// A rollback that wrote the version and then the hold could be interrupted
    /// between them, leaving an environment moved backwards with automatic
    /// advancement still live — so the next sweep would undo it, and the
    /// operator would watch their rollback disappear. One commit or neither.
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
    /// express a hold — that is what stops an automatic pass clearing one in
    /// order to succeed. These are what an *operator* calls. Two verbs with two
    /// callers beats one verb with a flag only one caller is trusted to set.
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
