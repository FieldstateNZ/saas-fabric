//! The test store's reservation rules: a frozen pending operation per
//! client, and a token for every attempt at it.

use super::{lock, InMemoryLifecycleContinuity};
use crate::{
    AcceptedLifecycle, ContinuityRefusal, ContinuityReservation, PendingTransition, Transition,
    TransitionKind,
};

impl InMemoryLifecycleContinuity {
    pub(super) fn admit(
        &self,
        transition: &Transition<'_>,
    ) -> Result<ContinuityReservation, ContinuityRefusal> {
        let mut clients = lock(&self.clients);
        let frozen = |reservation: ContinuityReservation| PendingTransition {
            reservation,
            kind: transition.kind,
            at_revision: transition.at_revision.cloned(),
            to: transition.to.clone(),
            to_record: transition.to_record.clone(),
            reversing: transition.reversing.cloned(),
            operator: transition.operator.to_owned(),
        };
        let token = || ContinuityReservation::new(format!("{}-{}", transition.client, self.next_operation()));
        if transition.kind == TransitionKind::Create {
            if clients.contains_key(transition.client) {
                return Err(ContinuityRefusal::Resurrection);
            }
            let reservation = token();
            let accepted = AcceptedLifecycle {
                instance: transition.instance.clone(),
                sequence: 0,
                fingerprint: transition.to.clone(),
                pending: Some(frozen(reservation.clone())),
            };
            clients.insert(transition.client.clone(), accepted);
            return Ok(reservation);
        }
        let accepted = clients
            .get_mut(transition.client)
            .ok_or(ContinuityRefusal::Unenrolled)?;
        let follows = *transition.instance == accepted.instance
            && transition.expected_sequence == Some(accepted.sequence)
            && transition.from == Some(&accepted.fingerprint)
            && (transition.kind != TransitionKind::Restore || transition.reversing.is_some());
        if let Some(held) = &accepted.pending {
            // Only exactly the frozen transition is handed back; anything
            // else, however close, is another operation.
            return if follows && *held == frozen(held.reservation.clone()) {
                Ok(held.reservation.clone())
            } else {
                Err(ContinuityRefusal::Pending)
            };
        }
        if !follows {
            return Err(ContinuityRefusal::Mismatch);
        }
        let reservation = token();
        accepted.pending = Some(frozen(reservation.clone()));
        Ok(reservation)
    }
    /// A new attempt at `operation`, outstanding until it completes the
    /// operation or is released as having written nothing.
    pub(super) fn attempt(&self, operation: &ContinuityReservation) -> ContinuityReservation {
        let mut attempts = lock(&self.attempts);
        let outstanding = attempts.entry(operation.id().to_owned()).or_default();
        let attempt = ContinuityReservation::new(format!("{}#{}", operation.id(), self.next_operation()));
        outstanding.insert(attempt.id().to_owned());
        attempt
    }
}

/// The operation an attempt token belongs to.
pub(super) fn operation_of(token: &ContinuityReservation) -> &str {
    token.id().split('#').next().unwrap_or_default()
}
