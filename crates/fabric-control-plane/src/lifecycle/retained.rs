//! Whether a tombstoned client's placements were retained, checked before it
//! is restored (D01-14 restore; #108 comments 6113749198 item 3 and
//! 6120741272 item 4).
//!
//! A tombstone retains every placement and the tenant stays published, so a
//! client restored as it was retired comes back on the same bindings. The
//! check compares the placements recorded now with what the publication
//! target holds, binding by binding, through the same O4 guard a pass
//! applies: a removed or replaced binding, or records below the held
//! revision or the tenant's resource-history frontier, refuse. So does a
//! placement for a logical source the client no longer declares, and a
//! tenant that history shows was published but the held document no longer
//! carries. Evidence that cannot be read refuses, and so does a target that
//! holds no accepted publication at all; "never published" passes only when
//! the target's own history establishes it.

use fabric_client_model::Client;
use fabric_core::TenantId;
use fabric_platform_management::{placements_retained, PlacementRecord, PlacementState as _};
use fabric_runtime_publication::PublishedResources;

use crate::state::ControlPlaneState;
use crate::RestoreRefusal;

/// What the check reads.
///
/// A deployment with no platform or no publication target is not evidence
/// that the client was never placed or published: its configuration could
/// have changed. Both are unknown retention, and refuse.
pub(crate) enum RetainedEvidence {
    /// This deployment manages no platform.
    NotManaged,
    /// The placements or the held publication could not be read.
    Unreadable,
    /// Every recorded placement, and what the target holds; `None` when no
    /// publication target is configured.
    Read {
        placements: Vec<PlacementRecord>,
        published: Option<PublishedResources>,
    },
}

impl ControlPlaneState {
    /// Reads the placements and the held publication once.
    pub(crate) async fn retained_evidence(&self) -> RetainedEvidence {
        let Some(platform) = &self.platform else {
            return RetainedEvidence::NotManaged;
        };
        let Ok(read) = platform.repository.read_placements(&platform.environment).await else {
            return RetainedEvidence::Unreadable;
        };
        let published = match &platform.publisher {
            None => None,
            Some(target) => match target.published_resources().await {
                Ok(published) => Some(published),
                Err(_) => return RetainedEvidence::Unreadable,
            },
        };
        RetainedEvidence::Read {
            placements: read.placements,
            published,
        }
    }
}

/// Refuses restoring `client` unless its placements are as it left them.
pub(crate) fn check(client: &Client, evidence: &RetainedEvidence) -> Result<(), RestoreRefusal> {
    let (placements, published) = match evidence {
        RetainedEvidence::NotManaged => return Err(RestoreRefusal::PlacementsUnverified),
        RetainedEvidence::Unreadable => return Err(RestoreRefusal::PlacementsUnreadable),
        RetainedEvidence::Read {
            placements,
            published,
        } => (placements, published),
    };
    let Ok(tenant) = TenantId::try_new(client.id.as_str()) else {
        return Ok(());
    };
    let records: Vec<_> = placements
        .iter()
        .filter(|record| record.tenant == tenant)
        .collect();
    if let Some(orphan) = records
        .iter()
        .find(|record| !client.data.contains_key(&record.logical))
    {
        return Err(not_retained(format!(
            "a placement records {}, which the client no longer declares",
            orphan.logical.as_str()
        )));
    }
    let Some(published) = published else {
        return Err(RestoreRefusal::PlacementsUnverified);
    };
    if published.tenants.is_empty() && published.history.tenants.is_empty() {
        return Err(RestoreRefusal::PlacementsUnverified);
    }
    let frontier = published.history.tenant(&tenant);
    let Some(held) = published.tenant(&tenant) else {
        return match frontier {
            None => Ok(()),
            Some(revision) => Err(not_retained(format!(
                "the tenant was published at revision {}, and the held tenants document no longer carries \
                 it",
                revision.get()
            ))),
        };
    };
    // A restore retains bindings exactly: a replaced data source or
    // isolation is refused even where a raised revision would admit it as a
    // new placement, and the O4 guard refuses anything removed or lowered.
    for (logical, binding) in &held.data {
        let same = records.iter().any(|record| {
            &record.logical == logical
                && record.data_source == binding.data_source
                && record.isolation == binding.isolation
        });
        if !same {
            return Err(not_retained(format!(
                "the tenant was published with {} bound as it is no longer recorded",
                logical.as_str()
            )));
        }
    }
    placements_retained(placements, std::slice::from_ref(held))
        .map_err(|error| not_retained(error.to_string()))?;

    let recorded = records
        .iter()
        .map(|record| record.revision.get())
        .fold(0, u64::saturating_add);
    match frontier {
        Some(revision) if recorded < revision.get() => Err(not_retained(format!(
            "the tenant's resource history reached revision {}, and its placements now add up to {recorded}",
            revision.get()
        ))),
        _ => Ok(()),
    }
}

fn not_retained(detail: String) -> RestoreRefusal {
    RestoreRefusal::PlacementsNotRetained { detail }
}
