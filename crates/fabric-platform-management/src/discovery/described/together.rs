//! Driving several registry reads at once, on the task that asked.

use std::future::{poll_fn, Future};
use std::pin::Pin;
use std::task::Poll;

/// One read, boxed so reads of different shapes share a list.
pub(crate) type Read<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The answers, in the order the reads were given, once every read has
/// answered or `settled` says the answers so far already decide.
///
/// A slot is `None` only for a read still in flight when `settled` said
/// so; that read is dropped before this returns.
///
/// # Why it can stop early
///
/// A caller reading the answers in its own order, the first failure
/// deciding, has its answer as soon as that failure and everything before
/// it are in — and waiting for the rest would let a read that hangs turn a
/// decided answer into a deadline reached, and a version that will never be
/// usable into "try again". `settled` is asked after every pass that
/// brought an answer in.
///
/// # Why polled here, and not spawned
///
/// The reads borrow the registry they ask, and a spawned task may not: it
/// would need the registry by `Arc`, and the rule's callers hand it in by
/// reference. Polling them together on the caller's task needs neither,
/// and keeps the property the catalogue's deadline depends on: dropping the
/// future this returns drops every read still in flight, so a resolution
/// cut off by its budget leaves nothing running behind it. Reads are safe
/// to abandon; nothing here writes.
///
/// Every read is polled on every wake, which is quadratic in the worst case
/// and irrelevant at the rule's bound of eight images, two reads each, and
/// at the handful of components an environment's desired state lists.
pub(crate) async fn together<T: Send>(
    reads: Vec<Read<'_, T>>,
    settled: impl Fn(&[Option<T>]) -> bool,
) -> Vec<Option<T>> {
    let mut pending: Vec<Option<Read<'_, T>>> = reads.into_iter().map(Some).collect();
    let mut answers: Vec<Option<T>> = pending.iter().map(|_| None).collect();

    poll_fn(|context| {
        let mut waiting = false;
        let mut arrived = false;
        for (slot, answer) in pending.iter_mut().zip(answers.iter_mut()) {
            let Some(read) = slot else { continue };
            match read.as_mut().poll(context) {
                Poll::Ready(value) => {
                    *answer = Some(value);
                    *slot = None;
                    arrived = true;
                }
                Poll::Pending => waiting = true,
            }
        }
        if waiting && !(arrived && settled(&answers)) {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;

    answers
}
