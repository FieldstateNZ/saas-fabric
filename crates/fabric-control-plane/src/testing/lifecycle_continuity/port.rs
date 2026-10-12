//! The test store's answers: one state machine over one map, so a
//! completion is only ever its own pending reservation's.
//!
//! In the 121-150 band: the port's four answers, kept together.

use async_trait::async_trait;
use fabric_client_model::{ClientId, ClientRevision};

use super::rules::operation_of;
use super::{lock, ContinuityEvent, InMemoryLifecycleContinuity};
use crate::{
    AcceptedLifecycle, ContinuityRefusal, ContinuityReservation, LifecycleContinuity, Transition,
    TransitionKind,
};

impl InMemoryLifecycleContinuity {
    fn reachable(&self) -> Result<(), ContinuityRefusal> {
        if *lock(&self.unavailable) {
            return Err(ContinuityRefusal::Unavailable {
                detail: "the test store is unreachable".to_owned(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl LifecycleContinuity for InMemoryLifecycleContinuity {
    async fn accepted(&self, client: &ClientId) -> Result<Option<AcceptedLifecycle>, ContinuityRefusal> {
        self.reachable()?;
        Ok(lock(&self.clients).get(client).cloned())
    }

    async fn reserve(&self, transition: &Transition<'_>) -> Result<ContinuityReservation, ContinuityRefusal> {
        self.reachable()?;
        if let Some(refusal) = lock(&self.refuse_with).clone() {
            return Err(refusal);
        }
        let operation = self.admit(transition)?;
        let reservation = self.attempt(&operation);
        if std::mem::take(&mut *lock(&self.concurrent_armed)) {
            *lock(&self.concurrent) = Some(self.attempt(&operation));
        }
        let reserved = ContinuityEvent::Reserved(
            transition.client.clone(),
            transition.kind,
            transition.at_revision.cloned(),
        );
        lock(&self.events).push(reserved);
        if let Some(hook) = lock(&self.on_reserve).as_ref() {
            hook();
        }
        Ok(reservation)
    }

    async fn complete(
        &self,
        reservation: &ContinuityReservation,
        revision: &ClientRevision,
    ) -> Result<(), ContinuityRefusal> {
        self.reachable()?;
        if *lock(&self.fail_completion) {
            return Err(ContinuityRefusal::Unavailable {
                detail: "the test store fails completions".to_owned(),
            });
        }
        let mut clients = lock(&self.clients);
        let (client, accepted) = clients
            .iter_mut()
            .find(|(_, accepted)| {
                accepted
                    .pending
                    .as_ref()
                    .is_some_and(|held| held.reservation.id() == operation_of(reservation))
            })
            .ok_or(ContinuityRefusal::StaleCompletion)?;
        let operation = operation_of(reservation).to_owned();
        let known = reservation.id() == operation
            || lock(&self.attempts)
                .get(&operation)
                .is_some_and(|outstanding| outstanding.contains(reservation.id()));
        if !known {
            return Err(ContinuityRefusal::StaleCompletion);
        }
        lock(&self.attempts).remove(&operation);
        if let Some(pending) = accepted.pending.take() {
            accepted.fingerprint = pending.to;
            accepted.sequence += 1;
        }
        lock(&self.events).push(ContinuityEvent::Completed(client.clone(), revision.clone()));
        Ok(())
    }

    async fn release(&self, reservation: &ContinuityReservation) {
        // Only this attempt is released. The operation stays pending while
        // any other attempt at it may have written.
        let operation = operation_of(reservation).to_owned();
        let remaining = {
            let mut attempts = lock(&self.attempts);
            let Some(outstanding) = attempts.get_mut(&operation) else {
                return;
            };
            if !outstanding.remove(reservation.id()) {
                return;
            }
            outstanding.len()
        };
        let mut clients = lock(&self.clients);
        let Some((client, kind)) = clients.iter().find_map(|(client, accepted)| {
            let pending = accepted.pending.as_ref()?;
            (pending.reservation.id() == operation).then(|| (client.clone(), pending.kind))
        }) else {
            return;
        };
        lock(&self.events).push(ContinuityEvent::Released(client.clone()));
        if remaining > 0 {
            return;
        }
        lock(&self.attempts).remove(&operation);
        if kind == TransitionKind::Create {
            clients.remove(&client);
        } else if let Some(accepted) = clients.get_mut(&client) {
            accepted.pending = None;
        }
    }
}
