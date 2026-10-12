//! Reserving one lifecycle transition against what the store accepted.

use fabric_client_model::lifecycle::Tombstone;
use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord};

use super::{
    ContinuityHold, ContinuityReservation, LifecycleContinuity, LifecycleFingerprint, Transition,
    TransitionKind,
};
use crate::logging;

/// One lifecycle write, as a service asks for it to be reserved.
pub(crate) struct Reserving<'a> {
    /// The client.
    pub(crate) client: &'a ClientId,
    /// Which write.
    pub(crate) kind: TransitionKind,
    /// The desired revision the operator reviewed; `None` for a creation.
    pub(crate) at_revision: Option<&'a ClientRevision>,
    /// The lifecycle the document records now; `None` for a creation.
    pub(crate) from: Option<&'a LifecycleRecord>,
    /// The lifecycle the write will record.
    pub(crate) to: &'a LifecycleRecord,
    /// For a restore, the tombstone it reverses.
    pub(crate) reversing: Option<&'a Tombstone>,
    /// The operator subject.
    pub(crate) operator: &'a str,
}

/// Reserves `write`, binding it to the instance and sequence the store
/// holds now, so a concurrent transition makes it a mismatch.
///
/// # Errors
///
/// The [`ContinuityHold`] the store's answer amounts to; its detail is
/// logged, never returned.
pub(crate) async fn reserve(
    continuity: &dyn LifecycleContinuity,
    write: &Reserving<'_>,
) -> Result<ContinuityReservation, ContinuityHold> {
    let (instance, expected_sequence) = if write.kind == TransitionKind::Create {
        (None, None)
    } else {
        match continuity.accepted(write.client).await {
            Ok(Some(accepted)) => (Some(accepted.instance), Some(accepted.sequence)),
            Ok(None) => return Err(refused(write.client, &super::ContinuityRefusal::Unenrolled)),
            Err(refusal) => return Err(refused(write.client, &refusal)),
        }
    };
    let from = write.from.map(LifecycleFingerprint::of);
    let to = LifecycleFingerprint::of(write.to);

    continuity
        .reserve(&Transition {
            client: write.client,
            kind: write.kind,
            instance,
            expected_sequence,
            at_revision: write.at_revision,
            from: from.as_ref(),
            to: &to,
            reversing: write.reversing,
            operator: write.operator,
        })
        .await
        .map_err(|refusal| refused(write.client, &refusal))
}

fn refused(client: &ClientId, refusal: &super::ContinuityRefusal) -> ContinuityHold {
    logging::continuity_refused(client, refusal);
    ContinuityHold::from_refusal(refusal)
}
