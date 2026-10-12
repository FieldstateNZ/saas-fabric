//! The test store's answers: one state machine over one map, so a
//! completion is only ever its own pending reservation's.

use async_trait::async_trait;
use fabric_client_model::{ClientId, ClientRevision};

use super::{lock, ContinuityEvent, InMemoryLifecycleContinuity};
use crate::{
    AcceptedLifecycle, ContinuityRefusal, ContinuityReservation, LifecycleContinuity, PendingTransition,
    Transition, TransitionKind,
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

    fn admit(&self, transition: &Transition<'_>) -> Result<ContinuityReservation, ContinuityRefusal> {
        let mut clients = lock(&self.clients);
        let reservation = ContinuityReservation::new(format!("{}-{:?}", transition.client, transition.kind));
        let pending = PendingTransition {
            reservation: reservation.clone(),
            to: transition.to.clone(),
            at_revision: transition.at_revision.cloned(),
            kind: transition.kind,
        };
        if transition.kind == TransitionKind::Create {
            if clients.contains_key(transition.client) {
                return Err(ContinuityRefusal::Resurrection);
            }
            let instance = self.next_instance();
            let fingerprint = transition.to.clone();
            let pending = Some(pending);
            clients.insert(
                transition.client.clone(),
                AcceptedLifecycle {
                    instance,
                    sequence: 0,
                    fingerprint,
                    pending,
                },
            );
            return Ok(reservation);
        }
        let accepted = clients
            .get_mut(transition.client)
            .ok_or(ContinuityRefusal::Unenrolled)?;
        if accepted
            .pending
            .as_ref()
            .is_some_and(|held| held.kind != pending.kind || held.at_revision != pending.at_revision)
        {
            return Err(ContinuityRefusal::Pending);
        }
        let follows = transition.instance == Some(accepted.instance)
            && transition.expected_sequence == Some(accepted.sequence)
            && transition.from == Some(&accepted.fingerprint)
            && (transition.kind != TransitionKind::Restore || transition.reversing.is_some());
        if !follows {
            return Err(ContinuityRefusal::Mismatch);
        }
        accepted.pending = Some(pending);
        Ok(reservation)
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
        let reservation = self.admit(transition)?;
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
                    .is_some_and(|held| held.reservation == *reservation)
            })
            .ok_or(ContinuityRefusal::StaleCompletion)?;
        if let Some(pending) = accepted.pending.take() {
            accepted.fingerprint = pending.to;
            accepted.sequence += 1;
        }
        lock(&self.events).push(ContinuityEvent::Completed(client.clone(), revision.clone()));
        Ok(())
    }

    async fn release(&self, reservation: &ContinuityReservation) {
        let mut clients = lock(&self.clients);
        let released = clients.iter().find_map(|(client, accepted)| {
            let pending = accepted.pending.as_ref()?;
            (pending.reservation == *reservation).then(|| (client.clone(), pending.kind))
        });
        if let Some((client, kind)) = released {
            if kind == TransitionKind::Create {
                clients.remove(&client);
            } else if let Some(accepted) = clients.get_mut(&client) {
                accepted.pending = None;
            }
            lock(&self.events).push(ContinuityEvent::Released(client));
        }
    }
}
