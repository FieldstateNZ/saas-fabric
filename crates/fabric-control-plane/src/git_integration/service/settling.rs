//! Running a whole transition to completion, whoever stops waiting for it.
//!
//! # Why a request must not own the transition
//!
//! Recording the integration and settling the live binding on it are one change
//! written to two places, and the second half is the slow one: pointing the
//! platform somewhere new waits for every operation still running against where
//! it used to point. Run inside an operator's request, that wait is cancellable
//! — the API's request timeout drops the handler future, and so does a browser
//! that goes away. A rebind dropped after the record was saved and while the
//! binding was still draining leaves the record naming the repository the
//! operator chose and the platform reading the one they replaced, until a
//! restart or some later rebind happens to notice.
//!
//! So the transition moves into a task, and the caller only *awaits* it.
//! Nothing a caller does can cancel a task it did not spawn: a caller that goes
//! away detaches the transition rather than stopping it. The operator may still
//! be told `504`; what has changed is that the platform converges anyway, and
//! asking again is safe rather than necessary.
//!
//! # Why the transitions are also ordered
//!
//! Two of them in flight could otherwise interleave — one saves `b`, the other
//! saves `c`, and they settle the binding in the other order, leaving the record
//! naming `c` and the platform reading `b`. The turn is held across the whole of
//! each, so each applies in full and the platform ends on whichever ran last.
//! Which of two overlapping requests that is, is whichever reached the turn
//! second; what cannot happen is neither.
//!
//! It is taken *inside* the task rather than before the spawn, because waiting a
//! turn has to be as uncancellable as the transition behind it. Acquired by the
//! caller, a request cut off while queued would take no turn at all, which is
//! the failure this exists to remove.
//!
//! # Why a bare order is not enough
//!
//! Ordering the writes leaves the reads in front of them unordered, and what
//! gives a transition the authority to write is precisely what it read. A
//! rebind reads the record and the private key, asks the host what the
//! installation reaches, and only then queues. A disconnect that takes its turn
//! inside that window unbinds, deletes the key and clears the record — and the
//! rebind, still holding both, saves the record again and binds with the key the
//! store no longer has. Two transitions, each applied in full and in order, and
//! the disconnect undone by authority captured before the order could see it.
//! The next restart finds a record with no key and reports an integration that
//! is connected and failing.
//!
//! So **anything whose validity depends on the current integration state is read
//! after the transition has its place in the order** — which, since the reads
//! have to happen first, means the state is instead *stamped*: the order carries
//! a generation, a request reads it before it reads anything else, hands it back
//! when it queues, and is refused if it moved in between. A disconnect that ran
//! first leaves the later rebind seeing a generation it does not recognise, and
//! it cannot resurrect what the disconnect forgot. See `order.rs`.
//!
//! Only the transitions that depend on a read pass one. A disconnect, a restore
//! and a creation pass [`None`] and each says why at its call site.

use std::future::Future;
use std::sync::Arc;

use crate::git_integration::service::order::Order;
use crate::git_integration::service::IntegrationError;
use crate::logging;

/// Runs one transition in a task of its own, in turn with every other.
///
/// `prepared_against` is the generation the caller observed before it read
/// anything the transition depends on, or [`None`] for a transition that
/// depends on nothing it read.
///
/// # What the task does not survive
///
/// It is spawned, so this needs a Tokio runtime, and a detached transition does
/// not outlive that runtime being dropped once graceful shutdown has returned.
/// A panic inside it is the other, and lands in the same place: the caller is
/// told the platform is unavailable, and nobody can say how far the transition
/// got — the record and the live binding may disagree, which is the one outcome
/// nothing else reports, and why it is logged at error.
///
/// # Errors
///
/// Whatever the transition returns; [`IntegrationError::Moved`] if the
/// integration moved on from `prepared_against`, in which case the transition
/// never runs; and [`IntegrationError::Unavailable`] if the task produced no
/// answer at all. Unavailable rather than a refusal: nothing about the request
/// was wrong, and this is not a transition observed to *fail* — it is one
/// nothing watched to the end.
pub(super) async fn settling<F>(
    order: Arc<Order>,
    prepared_against: Option<u64>,
    transition: F,
) -> Result<(), IntegrationError>
where
    F: Future<Output = Result<(), IntegrationError>> + Send + 'static,
{
    let running = tokio::spawn(async move { order.settle(prepared_against, transition).await });

    running.await.unwrap_or_else(|_joining| {
        logging::integration_transition_unobserved();
        Err(IntegrationError::Unavailable)
    })
}
