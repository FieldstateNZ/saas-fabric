//! Whether a tombstoned client's placements were retained, checked before it
//! is restored (D01-14 restore; #108 comment 6113749198, item 3).
//!
//! A tombstone retains every placement and the tenant stays published, so a
//! client restored as it was retired comes back on the same placements at
//! the same revisions. What this refuses is a client whose placements moved
//! while it was retired: a record that names a logical data source the
//! client no longer declares, or records that no longer add up to the
//! revision the tenant was last published at, which only a removed or
//! lowered record produces. Absence of evidence refuses too: placements that
//! cannot be read, or a process that has not seen a pass publish. Pure, so
//! each rule is a unit test.

use fabric_client_model::Client;
use fabric_core::TenantId;

use super::facts::{Placements, PlatformFacts, Publication};
use crate::RestoreRefusal;

/// Refuses restoring `client` unless its placements are as it left them.
///
/// A deployment that manages no platform has no placements to retain, and
/// a tenant that was never placed has none either; each passes, and its
/// readiness says what is missing.
pub(crate) fn check(client: &Client, facts: &PlatformFacts) -> Result<(), RestoreRefusal> {
    let placements = match &facts.placements {
        Placements::NotManaged => return Ok(()),
        Placements::Unreadable => return Err(RestoreRefusal::PlacementsUnreadable),
        Placements::Read(placements) => placements,
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

    let published = match &facts.publication {
        Publication::NotConfigured => return Ok(()),
        Publication::Observed { published: None, .. } => return Err(RestoreRefusal::PlacementsUnverified),
        Publication::Observed {
            published: Some(published),
            ..
        } => published.tenants.get(&tenant).copied(),
        Publication::Offered(tenants) => tenants.get(&tenant).copied(),
    };
    let Some(published) = published else {
        return Ok(());
    };
    let recorded = records
        .iter()
        .map(|record| record.revision.get())
        .fold(0, u64::saturating_add);

    if recorded < published.get() {
        return Err(not_retained(format!(
            "the tenant was last published at revision {}, and its placements now add up to {recorded}: a \
             placement was removed or lowered while the client was retired",
            published.get()
        )));
    }
    Ok(())
}

fn not_retained(detail: String) -> RestoreRefusal {
    RestoreRefusal::PlacementsNotRetained { detail }
}
