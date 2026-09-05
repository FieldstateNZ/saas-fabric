//! Whose turn it is to change the integration, and which one they prepared for.
//!
//! Over the advisory size on purpose: the turn and the compare under it are
//! one mechanism, and the counter kept anywhere but beside the turn it is
//! compared under would be one nobody could trust.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Mutex;

use crate::git_integration::service::IntegrationError;
use crate::logging;

mod bumped;
mod generation;

use bumped::Bumped;
pub(super) use generation::Generation;

/// One transition at a time, each admitted against the state it prepared
/// against.
///
/// Why both halves exist is the argument in `settling.rs`: the turn keeps two
/// transitions from interleaving, and the generation keeps one from running on
/// authority it captured before it had a turn. One type, because a generation
/// kept anywhere but beside the turn it is compared under is a counter nobody
/// could trust.
///
/// Default is an order nobody has taken a turn in yet, at generation zero.
///
/// The order is one process's. A second control-plane replica would share
/// neither the turn nor the generation, and the compare orders a local counter
/// against a record and a key read from the secret store — so what this
/// guarantees holds within one control plane, and rests on that store reading
/// its own writes.
#[derive(Default)]
pub(super) struct Order {
    /// Held for the whole of one transition.
    ///
    /// An async lock rather than [`std::sync::Mutex`]: it is held across the
    /// await that drains the binding, and a blocking guard held across an await
    /// point is how a runtime deadlocks.
    turn: Mutex<()>,

    /// How many transitions have landed.
    ///
    /// Nothing reads it as a count: it exists only to be unequal to its
    /// predecessor, so wrapping is harmless — [`AtomicU64::fetch_add`] wraps
    /// rather than panicking, and [`u64::MAX`] is further off than the platform.
    generation: AtomicU64,
}

impl Order {
    /// Which generation the integration is on, without queueing to find out.
    ///
    /// Deliberately lock-free: a request that is only *preparing* has nothing
    /// to serialise with, and waiting a turn to read a number would put every
    /// preparation behind whatever drain is running. The answer may be stale by
    /// the time it returns, which is the point — this is not the check. The
    /// check is the compare in [`settle`](Self::settle), under the turn. Read
    /// *first*, staleness runs one way only: too old, never too new, and too
    /// old is a refusal rather than a resurrection.
    pub(super) fn observed(&self) -> Generation {
        Generation::minted(self.generation.load(Ordering::SeqCst))
    }

    /// Whether a transition holds the order right now.
    ///
    /// For tests only: a test about ordering has to see the order engaged while
    /// a transition is parked, and nothing in production has a reason to ask.
    #[cfg(test)]
    pub(super) fn is_held(&self) -> bool {
        self.turn.try_lock().is_err()
    }

    /// Runs one transition in turn, refusing one whose state has since moved.
    ///
    /// `prepared_against` is the generation the caller read before it read
    /// anything else. [`None`] means the transition depends on nothing it read
    /// and is admitted whatever landed first; each call site says why.
    ///
    /// # Why the compare is under the turn
    ///
    /// It is this transition's admission test, and it has to hold for as long
    /// as the transition takes. Compared before taking the turn, another could
    /// land in between and admit the stale request anyway — the same race one
    /// level down. The turn is what makes "the generation is `g`" and "this
    /// transition runs next" one fact rather than two.
    ///
    /// # Why the bump follows the transition, however it ended
    ///
    /// The generation counts transitions that *ran*, not ones that returned
    /// `Ok`. A transition that failed part-way has still moved the state — a
    /// disconnect whose key deletion succeeded and whose record clear did not
    /// has left an integration nothing can use — and a rebind prepared before
    /// it must not be admitted to put the record back on the strength of what
    /// it read. So anything that ran moves the generation: returned, failed,
    /// panicked, or dropped with its runtime, which is why the bump is a guard
    /// and not a line after the await. The cost is a refusal for a request
    /// prepared alongside a transition that then failed harmlessly; the
    /// operator looks again at a page that may not have changed, which is
    /// cheaper than the alternative in every case it differs.
    ///
    /// A transition that was refused for being stale did not run, and moves
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`IntegrationError::Moved`] if the generation moved, in which case the
    /// transition is **not run** — nothing landed, so nothing accounts for one
    /// having landed. Otherwise whatever the transition itself returns.
    pub(super) async fn settle<F>(
        &self,
        prepared_against: Option<Generation>,
        transition: F,
    ) -> Result<(), IntegrationError>
    where
        F: Future<Output = Result<(), IntegrationError>>,
    {
        let held = self.turn.lock().await;

        if prepared_against.is_some_and(|generation| generation != self.observed()) {
            logging::integration_transition_moved();
            return Err(IntegrationError::Moved);
        }

        // From here the transition runs, so from here it may have moved the
        // state — and the guard says so whether it returns, fails, panics or
        // is dropped with its runtime.
        let bumped = Bumped(&self.generation);

        let outcome = transition.await;

        // The bump, then the release, not the other way round: releasing the
        // turn is the signal that this one has been written in full, and that
        // whoever prepared against the generation it left is now stale.
        drop(bumped);
        drop(held);

        outcome
    }
}
