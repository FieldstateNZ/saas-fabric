//! Whose turn it is to change the integration, and which one they prepared for.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Mutex;

use crate::git_integration::service::IntegrationError;
use crate::logging;

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
    pub(super) fn observed(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
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
    /// # Why the bump waits for the transition to succeed
    ///
    /// The generation counts what *landed*. Bumping on the way in would refuse
    /// everything prepared alongside a transition that then refused or failed,
    /// sending operators to look again at a page that had not changed.
    ///
    /// The residual is one that failed part-way: a disconnect whose key
    /// deletion succeeded and whose record clear did not has moved the state
    /// without moving the generation, so a rebind prepared before it is still
    /// admitted and can put the record back. That disconnect was reported
    /// failed, which is the honest half of it, and asking again closes it.
    ///
    /// # Errors
    ///
    /// [`IntegrationError::Moved`] if the generation moved, in which case the
    /// transition is **not run** — nothing landed, so nothing accounts for one
    /// having landed. Otherwise whatever the transition itself returns.
    pub(super) async fn settle<F>(
        &self,
        prepared_against: Option<u64>,
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

        let outcome = transition.await;

        if outcome.is_ok() {
            self.generation.fetch_add(1, Ordering::SeqCst);
        }

        // After the transition and the bump, not before either: releasing the
        // turn is the signal that this one has been written in full, and that
        // whoever prepared against the generation it left is now stale.
        drop(held);

        outcome
    }
}
