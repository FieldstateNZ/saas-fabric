//! A process-only stand-in for the protected continuity store, for tests. It
//! proves classification and order, nothing else: it forgets all on restart,
//! coordinates no processes, and no deployment builds it (#108).
//!
//! The controls are here; the port and its rules are in `port`.
//!
//! In the 121-150 band: the store and the controls a test turns.

use std::collections::BTreeMap;
use std::sync::Mutex;

use fabric_client_model::lifecycle::IdentityInstance;
use fabric_client_model::{ClientId, ClientRevision};

use crate::{
    AcceptedLifecycle, ContinuityRefusal, ContinuityReservation, LifecycleFingerprint, TransitionKind,
};

mod port;
#[cfg(test)]
mod port_tests;
mod rules;

/// What happened, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuityEvent {
    /// A transition reserved, with the revision it was reviewed at.
    Reserved(ClientId, TransitionKind, Option<ClientRevision>),
    /// Completed at the revision the write produced.
    Completed(ClientId, ClientRevision),
    /// Released because the write did not happen.
    Released(ClientId),
}

/// Runs inside a reservation, standing in for a concurrent writer.
type ReserveHook = Box<dyn Fn() + Send + Sync>;

/// Enrolls only what a test enrolls or an API creation reserves.
#[derive(Default)]
pub struct InMemoryLifecycleContinuity {
    clients: Mutex<BTreeMap<ClientId, AcceptedLifecycle>>,
    operations: Mutex<u64>,
    refuse_with: Mutex<Option<ContinuityRefusal>>,
    unavailable: Mutex<bool>,
    fail_completion: Mutex<bool>,
    on_reserve: Mutex<Option<ReserveHook>>,
    events: Mutex<Vec<ContinuityEvent>>,
    /// Each pending operation's attempts whose outcome is not known to be
    /// "nothing written".
    attempts: Mutex<BTreeMap<String, std::collections::BTreeSet<String>>>,
    concurrent_armed: Mutex<bool>,
    concurrent: Mutex<Option<ContinuityReservation>>,
}

impl InMemoryLifecycleContinuity {
    /// A store with nothing enrolled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enrolls `client` as `instance`, accepting `fingerprint`: the reviewed
    /// enrollment a test stands in for.
    pub fn enroll(&self, client: &ClientId, instance: IdentityInstance, fingerprint: LifecycleFingerprint) {
        lock(&self.clients).insert(
            client.clone(),
            AcceptedLifecycle {
                instance,
                sequence: 0,
                fingerprint,
                pending: None,
            },
        );
    }

    /// Refuses every later reservation with `refusal`.
    pub fn refuse_with(&self, refusal: ContinuityRefusal) {
        *lock(&self.refuse_with) = Some(refusal);
    }

    /// Makes every later call fail as unreachable, or stops it.
    pub fn set_unavailable(&self, unavailable: bool) {
        *lock(&self.unavailable) = unavailable;
    }

    /// Fails, or stops failing, every later completion.
    pub fn fail_completions(&self, failing: bool) {
        *lock(&self.fail_completion) = failing;
    }

    /// Makes the next successful reservation also hand out a second attempt
    /// at the same operation, as an identical concurrent request would.
    pub fn reserve_concurrently(&self) {
        *lock(&self.concurrent_armed) = true;
    }

    /// The second attempt [`Self::reserve_concurrently`] handed out.
    #[must_use]
    pub fn concurrent_attempt(&self) -> Option<ContinuityReservation> {
        lock(&self.concurrent).clone()
    }

    /// Runs `hook` inside every later successful reservation.
    pub fn on_reserve(&self, hook: impl Fn() + Send + Sync + 'static) {
        *lock(&self.on_reserve) = Some(Box::new(hook));
    }

    /// Every call so far, in order.
    #[must_use]
    pub fn events(&self) -> Vec<ContinuityEvent> {
        lock(&self.events).clone()
    }

    /// A token no earlier operation used.
    pub(super) fn next_operation(&self) -> u64 {
        let mut operations = lock(&self.operations);
        *operations += 1;
        *operations
    }
}

pub(super) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
