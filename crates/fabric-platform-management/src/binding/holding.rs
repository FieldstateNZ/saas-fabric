//! Holding the binding for as long as the operation, not the caller.

use std::future::Future;
use std::sync::Arc;

use tokio::sync::OwnedRwLockReadGuard;

use crate::binding::live::Live;
use crate::binding::{generation, PlatformDesiredState};
use crate::{DesiredRevision, DesiredState, DesiredStateError};

impl PlatformDesiredState {
    /// The binding, in a guard that can be handed to a task.
    ///
    /// Owned rather than borrowed. A borrowed guard is tied to the borrow of
    /// `self` that produced it, so it cannot be moved into the task the
    /// operation runs in — and that task is the whole point.
    pub(super) async fn held(&self) -> OwnedRwLockReadGuard<Live> {
        Arc::clone(&self.current).read_owned().await
    }

    /// Everything a write needs before it can be handed to a task.
    ///
    /// The guard, the repository, the adapter's own revision with this
    /// binding's tag stripped off it, and owned copies of the two names — owned
    /// because the task outlives the borrow they arrived as.
    ///
    /// The repository is resolved **before** the tag is checked, and that order
    /// is deliberate: a platform with nothing bound answers `NotConnected`,
    /// which is a state an operator acts on, rather than `Conflict`, which
    /// would send them to retry something there is nothing to retry against.
    ///
    /// # Errors
    ///
    /// [`NotConnected`](DesiredStateError::NotConnected) or
    /// [`Unavailable`](DesiredStateError::Unavailable) if nothing usable is
    /// bound, and [`Conflict`](DesiredStateError::Conflict) if the decision was
    /// read through a different binding.
    pub(super) async fn writing(
        &self,
        environment: &str,
        component: &str,
        at: &DesiredRevision,
    ) -> Result<Writing, DesiredStateError> {
        let live = self.held().await;
        let repository = live.repository()?;
        let at = generation::untag(live.generation(), at)?;

        Ok((live, repository, environment.to_owned(), component.to_owned(), at))
    }
}

/// What [`writing`](PlatformDesiredState::writing) hands back.
///
/// A named alias rather than the tuple spelled out at every call site: the
/// shape is five things a write cannot start without, and writing it five times
/// would say nothing the name does not.
pub(super) type Writing = (
    OwnedRwLockReadGuard<Live>,
    Arc<dyn DesiredState>,
    String,
    String,
    DesiredRevision,
);

/// Runs one delegated operation in a task that owns the read guard.
///
/// # Why the caller's own future will not do
///
/// Holding the guard across the delegated await is enough while the caller
/// stays. It is not enough when the caller goes away: an axum handler cut off
/// by the request timeout, or one whose browser closed, has its future dropped,
/// and a guard dropped with it releases the binding while the operation's last
/// request may already be on the wire. A disconnect could then return — telling
/// an operator the platform had stopped writing to that repository — and the
/// abandoned write land in it afterwards.
///
/// A task is not cancelled by the disappearance of whoever spawned it. So the
/// operation, the arguments it was given and the guard all move into one, the
/// delegate simply waits for it, and a caller that stops waiting changes
/// nothing about what the platform finishes doing or about what the drain
/// waits for.
///
/// # Errors
///
/// Whatever the operation returns, and
/// [`Unavailable`](DesiredStateError::Unavailable) if the task did not produce
/// an answer at all — it panicked, or the runtime is shutting down. Unavailable
/// rather than a refusal, because nothing about the request was wrong and
/// nobody can say whether the write landed.
pub(super) async fn outliving<T, F>(
    guard: OwnedRwLockReadGuard<Live>,
    operation: F,
) -> Result<T, DesiredStateError>
where
    F: Future<Output = Result<T, DesiredStateError>> + Send + 'static,
    T: Send + 'static,
{
    let running = tokio::spawn(async move {
        let outcome = operation.await;

        // After the operation and not before it: releasing the binding is the
        // signal that this repository is finished with.
        drop(guard);

        outcome
    });

    running.await.unwrap_or_else(|_joining| {
        Err(DesiredStateError::Unavailable {
            detail: "the platform operation ended without an answer".to_owned(),
        })
    })
}
