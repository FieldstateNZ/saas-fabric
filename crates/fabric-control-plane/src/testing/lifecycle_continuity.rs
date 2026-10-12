//! A process-only stand-in for the protected continuity store, for tests. It
//! proves the order a restore writes in and nothing else: it forgets all on
//! restart, coordinates no processes, and no deployment builds it (#108).

use std::collections::BTreeSet;
use std::sync::Mutex;

use async_trait::async_trait;
use fabric_client_model::{ClientId, ClientRevision};

use crate::{ContinuityRefusal, ContinuityReservation, LifecycleContinuity, Restoring};

/// What happened to one reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinuityEvent {
    /// Reserved for a client, from the revision it was read at.
    Reserved(ClientId, ClientRevision),
    /// Completed at the revision the write produced.
    Completed(ClientId, ClientRevision),
    /// Released because the write did not happen.
    Released(ClientId),
}

/// Runs while a reservation is being made, standing in for a concurrent
/// writer that moves the document between the reservation and the write.
type ReserveHook = Box<dyn Fn() + Send + Sync>;

/// Records every call in order. Clients are enrolled explicitly; anything
/// else is refused as unenrolled, as an adapter must.
#[derive(Default)]
pub struct InMemoryLifecycleContinuity {
    enrolled: Mutex<BTreeSet<ClientId>>,
    refuse_with: Mutex<Option<ContinuityRefusal>>,
    fail_completion: Mutex<bool>,
    on_reserve: Mutex<Option<ReserveHook>>,
    events: Mutex<Vec<ContinuityEvent>>,
}

impl InMemoryLifecycleContinuity {
    /// A store with nothing enrolled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enrolls `client`, so its restorations can be reserved.
    pub fn enroll(&self, client: &ClientId) {
        lock(&self.enrolled).insert(client.clone());
    }

    /// Refuses every later reservation with `refusal`.
    pub fn refuse_with(&self, refusal: ContinuityRefusal) {
        *lock(&self.refuse_with) = Some(refusal);
    }

    /// Fails every later completion.
    pub fn fail_completions(&self) {
        *lock(&self.fail_completion) = true;
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
}

#[async_trait]
impl LifecycleContinuity for InMemoryLifecycleContinuity {
    async fn reserve(&self, restoration: &Restoring<'_>) -> Result<ContinuityReservation, ContinuityRefusal> {
        if let Some(refusal) = lock(&self.refuse_with).clone() {
            return Err(refusal);
        }
        if !lock(&self.enrolled).contains(restoration.client) {
            return Err(ContinuityRefusal::Unenrolled);
        }
        if restoration.from.tombstone.is_none() || restoration.to.tombstone.is_some() {
            return Err(ContinuityRefusal::Mismatch);
        }
        lock(&self.events).push(ContinuityEvent::Reserved(
            restoration.client.clone(),
            restoration.at_revision.clone(),
        ));
        if let Some(hook) = lock(&self.on_reserve).as_ref() {
            hook();
        }
        Ok(ContinuityReservation::new(restoration.client.as_str()))
    }

    async fn complete(
        &self,
        reservation: &ContinuityReservation,
        revision: &ClientRevision,
    ) -> Result<(), ContinuityRefusal> {
        if *lock(&self.fail_completion) {
            return Err(ContinuityRefusal::Unavailable {
                detail: "the test store fails completions".to_owned(),
            });
        }
        let client = ClientId::try_new(reservation.id()).map_err(|_| ContinuityRefusal::Mismatch)?;
        lock(&self.events).push(ContinuityEvent::Completed(client, revision.clone()));
        Ok(())
    }

    async fn release(&self, reservation: &ContinuityReservation) {
        if let Ok(client) = ClientId::try_new(reservation.id()) {
            lock(&self.events).push(ContinuityEvent::Released(client));
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
