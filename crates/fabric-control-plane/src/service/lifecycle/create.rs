//! Reserving a new identity instance for a client being created (O5, #108
//! comment 6120741272 item 2).
//!
//! Creation stages desired intent whatever the store says. What it cannot do
//! without the store is admit an identity: only a completed new-instance
//! reservation proves the id is not a retired identity coming back. A
//! granted reservation writes the minted instance into `spec.lifecycle`; a
//! refused one is logged and the document is staged with no instance, so it
//! is never the identity the store knows and stays held.

use fabric_client_model::{ClientDocument, ClientId, ClientRevision};

use crate::continuity::{self, Reserving};
use crate::TransitionKind;
use crate::{
    logging, ClientService, ContinuityReservation, ControlPlaneError, LifecycleContinuity, Operator,
};

impl ClientService {
    /// `document` with a freshly minted identity instance and its
    /// new-instance reservation, when there is a store and it grants one;
    /// otherwise `document` as it is, with none.
    pub(crate) async fn reserve_creation(
        &self,
        continuity: Option<&dyn LifecycleContinuity>,
        document: ClientDocument,
        operator: &Operator,
    ) -> (ClientDocument, Option<ContinuityReservation>) {
        let (Some(continuity), Some(instance)) = (continuity, continuity::mint()) else {
            return (document, None);
        };
        let mut lifecycle = document.lifecycle().clone();
        lifecycle.instance = Some(instance);
        let Ok(attested) = document.with_lifecycle(&lifecycle) else {
            return (document, None);
        };
        let client = document.client();
        let reserved = continuity::reserve(
            continuity,
            &Reserving {
                client: &client.id,
                realm: &client.identity.realm,
                kind: TransitionKind::Create,
                at_revision: None,
                from: None,
                to: &lifecycle,
                reversing: None,
                operator: operator.subject(),
            },
        )
        .await;
        match reserved {
            Ok(reservation) => (attested, Some(reservation)),
            Err(_) => (document, None),
        }
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
