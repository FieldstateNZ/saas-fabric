//! Running a whole transition to completion, whoever stops waiting for it.

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::git_integration::service::IntegrationError;
use crate::logging;

/// Runs one transition in a task of its own, in turn with every other.
///
/// # Why a request must not own the transition
///
/// Recording the integration and settling the live binding on it are one
/// change written to two places, and the second half is the slow one: pointing
/// the platform somewhere new waits for every operation still running against
/// where it used to point. Run inside an operator's request, that wait is
/// cancellable — the API's request timeout drops the handler future, and so
/// does a browser that goes away. A rebind dropped after the record was saved
/// and while the binding was still draining leaves the record naming the
/// repository the operator chose and the platform reading the one they
/// replaced, until a restart or some later rebind happens to notice.
///
/// So the transition moves into a task, and the caller only *awaits* it.
/// Nothing a caller does can cancel a task it did not spawn: a caller that
/// goes away detaches the transition rather than stopping it. The operator may
/// still be told `504`; what has changed is that the platform converges anyway,
/// and asking again is safe rather than necessary.
///
/// # Why the transitions are also ordered
///
/// Two of them in flight could otherwise interleave — one saves B, the other
/// saves C, and they settle the binding in the other order, leaving the record
/// naming C and the platform reading B. The lock is held across the whole of
/// each, so each applies in full and the platform ends on whichever ran last.
/// Which of two overlapping requests that is, is whichever reached the lock
/// second; what cannot happen is neither.
///
/// It is taken *inside* the task rather than before the spawn, because waiting
/// a turn has to be as uncancellable as the transition behind it. Acquired by
/// the caller, a request cut off while queued would take no turn at all, which
/// is the failure this exists to remove.
///
/// # What the task does not survive
///
/// It is spawned, so this needs a Tokio runtime, and a detached transition does
/// not outlive that runtime being dropped once graceful shutdown has returned.
/// A panic inside it is the other, and lands in the same place: the caller is
/// told the platform is unavailable, and nobody can say how far the transition
/// got — the record and the live binding may disagree, which is the one
/// outcome nothing else reports, and why it is logged at error.
///
/// # Errors
///
/// Whatever the transition returns, and [`IntegrationError::Unavailable`] if
/// the task produced no answer at all. Unavailable rather than a refusal:
/// nothing about the request was wrong, and this is not a transition observed
/// to *fail* — it is one nothing watched to the end.
pub(super) async fn settling<F>(order: Arc<Mutex<()>>, transition: F) -> Result<(), IntegrationError>
where
    F: Future<Output = Result<(), IntegrationError>> + Send + 'static,
{
    let running = tokio::spawn(async move {
        let ordered = order.lock().await;
        let outcome = transition.await;

        // After the transition and not before it: releasing the order is the
        // signal that both halves of this one have been written.
        drop(ordered);

        outcome
    });

    running.await.unwrap_or_else(|_joining| {
        logging::integration_transition_unobserved();
        Err(IntegrationError::Unavailable)
    })
}
