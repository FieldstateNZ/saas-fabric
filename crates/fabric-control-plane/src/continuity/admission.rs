//! Whether the continuity store confirms a stored client's lifecycle: the
//! guard every identity side effect and readiness decision passes.

use std::collections::BTreeMap;

use fabric_client_model::ClientId;

use super::{ContinuityHold, LifecycleContinuity, LifecycleFingerprint};
use crate::StoredClient;

/// What the store says about one stored client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admission {
    /// The document's lifecycle is the one the store accepted, with nothing
    /// pending.
    Admitted,
    /// It is not, or nothing shows that it is.
    Held(ContinuityHold),
}

/// Admits `stored` only when the store's accepted lifecycle, with nothing
/// pending, has exactly the document's fingerprint.
///
/// Every identity instance is checked, a client with no lifecycle history
/// included: conditioning on a visible tombstone or restoration would be
/// bypassed by erasing it.
pub(crate) async fn admit(continuity: Option<&dyn LifecycleContinuity>, stored: &StoredClient) -> Admission {
    let Some(continuity) = continuity else {
        return Admission::Held(ContinuityHold::NoStore);
    };
    let client = &stored.document.client().id;
    let accepted = match continuity.accepted(client).await {
        Ok(Some(accepted)) => accepted,
        Ok(None) => return Admission::Held(ContinuityHold::Unenrolled),
        Err(refusal) => return Admission::Held(ContinuityHold::from_refusal(&refusal)),
    };
    if accepted.pending.is_some() {
        return Admission::Held(ContinuityHold::Pending);
    }
    if accepted.fingerprint == LifecycleFingerprint::of(stored.document.lifecycle()) {
        Admission::Admitted
    } else {
        Admission::Held(ContinuityHold::Mismatch)
    }
}

/// [`admit`] for each of `clients`, by id.
pub(crate) async fn admit_all(
    continuity: Option<&dyn LifecycleContinuity>,
    clients: &[StoredClient],
) -> BTreeMap<ClientId, Admission> {
    let mut admissions = BTreeMap::new();
    for stored in clients {
        admissions.insert(
            stored.document.client().id.clone(),
            admit(continuity, stored).await,
        );
    }
    admissions
}
