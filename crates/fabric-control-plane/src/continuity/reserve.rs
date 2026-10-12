//! Reserving one lifecycle transition against what the store accepted, and
//! minting the identity instance a creation reserves.

use fabric_client_model::lifecycle::{IdentityInstance, Tombstone};
use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord, RealmName};
use ring::rand::SecureRandom as _;

use super::{
    ContinuityHold, ContinuityRefusal, ContinuityReservation, LifecycleContinuity, LifecycleFingerprint,
    Transition, TransitionKind,
};
use crate::logging;

/// One lifecycle write, as a service asks for it to be reserved.
pub(crate) struct Reserving<'a> {
    /// The client.
    pub(crate) client: &'a ClientId,
    /// Its realm: part of the identity context the fingerprint binds.
    pub(crate) realm: &'a RealmName,
    /// Which write.
    pub(crate) kind: TransitionKind,
    /// The desired revision the operator reviewed; `None` for a creation.
    pub(crate) at_revision: Option<&'a ClientRevision>,
    /// The lifecycle the document records now; `None` for a creation.
    pub(crate) from: Option<&'a LifecycleRecord>,
    /// The exact lifecycle the write will record.
    pub(crate) to: &'a LifecycleRecord,
    /// For a restore, the tombstone it reverses.
    pub(crate) reversing: Option<&'a Tombstone>,
    /// The operator subject.
    pub(crate) operator: &'a str,
}

/// A fresh identity instance, from the system's secure random source, or
/// `None` when that source fails: a creation then reserves nothing.
pub(crate) fn mint() -> Option<IdentityInstance> {
    let mut bytes = [0_u8; 16];
    ring::rand::SystemRandom::new().fill(&mut bytes).ok()?;
    let text = bytes.iter().fold(String::new(), |mut text, byte| {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
        text
    });
    IdentityInstance::try_new(&text).ok()
}

/// Reserves `write`, binding it to the instance and sequence the store
/// holds now, so a concurrent transition makes it a mismatch. A creation
/// binds the instance in `write.to`.
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
        let Some(instance) = write.to.instance.clone() else {
            return Err(ContinuityHold::Unenrolled);
        };
        (instance, None)
    } else {
        match continuity.accepted(write.client).await {
            Ok(Some(accepted)) => (accepted.instance, Some(accepted.sequence)),
            Ok(None) => return Err(refused(write.client, &ContinuityRefusal::Unenrolled)),
            Err(refusal) => return Err(refused(write.client, &refusal)),
        }
    };
    let from = write
        .from
        .map(|from| LifecycleFingerprint::of(write.client, write.realm, from));
    let to = LifecycleFingerprint::of(write.client, write.realm, write.to);

    continuity
        .reserve(&Transition {
            client: write.client,
            kind: write.kind,
            instance: &instance,
            expected_sequence,
            at_revision: write.at_revision,
            from: from.as_ref(),
            to: &to,
            to_record: write.to,
            reversing: write.reversing,
            operator: write.operator,
        })
        .await
        .map_err(|refusal| refused(write.client, &refusal))
}

fn refused(client: &ClientId, refusal: &ContinuityRefusal) -> ContinuityHold {
    logging::continuity_refused(client, refusal);
    ContinuityHold::from_refusal(refusal)
}
