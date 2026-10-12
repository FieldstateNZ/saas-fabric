//! Reserving a new identity instance for a client being created (O5, #108
//! comment 6120741272 item 2).
//!
//! Creation stages desired intent whatever the store says. What it cannot do
//! without the store is admit an identity: only a completed new-instance
//! reservation proves the id is not a retired identity coming back. A
//! refused reservation is logged and the document is still written; the
//! client then stays held, as every client does without a store.

use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord};

use crate::continuity::{self, Reserving};
use crate::TransitionKind;
use crate::{
    logging, ClientService, ContinuityReservation, ControlPlaneError, LifecycleContinuity, Operator,
};

impl ClientService {
    /// A new-instance reservation for `id`, when there is a store and it
    /// grants one.
    pub(crate) async fn reserve_creation(
        &self,
        continuity: Option<&dyn LifecycleContinuity>,
        id: &ClientId,
        lifecycle: &LifecycleRecord,
        operator: &Operator,
    ) -> Option<ContinuityReservation> {
        let continuity = continuity?;
        continuity::reserve(
            continuity,
            &Reserving {
                client: id,
                kind: TransitionKind::Create,
                at_revision: None,
                from: None,
                to: lifecycle,
                reversing: None,
                operator: operator.subject(),
            },
        )
        .await
        .ok()
    }

    /// Completes, releases or leaves pending `reservation`, by how the
    /// creation's document write ended.
    pub(crate) async fn settle_creation(
        &self,
        continuity: Option<&dyn LifecycleContinuity>,
        id: &ClientId,
        reservation: Option<ContinuityReservation>,
        written: Result<&ClientRevision, &ControlPlaneError>,
    ) {
        let (Some(continuity), Some(reservation)) = (continuity, reservation) else {
            return;
        };
        match written {
            Ok(revision) => {
                if let Err(refusal) = continuity.complete(&reservation, revision).await {
                    logging::continuity_completion_failed(id, reservation.id(), &refusal);
                }
            }
            Err(ControlPlaneError::ClientExists { .. } | ControlPlaneError::RevisionConflict) => {
                continuity.release(&reservation).await;
            }
            Err(_) => logging::continuity_write_unconfirmed(id, reservation.id()),
        }
    }
}
