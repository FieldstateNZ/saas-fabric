//! The evidence for each readiness condition, from facts the control plane
//! already holds (D01-19a). Pure, so each rule is a unit test.
//!
//! Absence of evidence is never readiness: a platform this deployment does
//! not manage, a placements document that could not be read, or a process
//! that has not completed a publication pass since it started all leave the
//! condition unmet, with the reason.

use fabric_client_model::lifecycle::Evidence;
use fabric_client_model::{Client, ProjectionCheck};
use fabric_core::TenantId;
use fabric_reconciliation::ReconciliationStatus;

use super::facts::{Placements, PlatformFacts, Publication};

/// Every logical data source the client declares has a placement.
pub(crate) fn provisioning(client: &Client, facts: &PlatformFacts) -> Evidence {
    let placements = match &facts.placements {
        Placements::NotManaged => return unmet("this deployment manages no platform to place data on"),
        Placements::Unreadable => return unmet("the environment's placements could not be read"),
        Placements::Read(placements) => placements,
    };
    if client.data.is_empty() {
        return unmet("the client declares no data to place");
    }
    let missing: Vec<&str> = client
        .data
        .keys()
        .filter(|logical| {
            !placements
                .iter()
                .any(|record| record.tenant.as_str() == client.id.as_str() && &record.logical == *logical)
        })
        .map(fabric_core::LogicalDataSourceName::as_str)
        .collect();

    if missing.is_empty() {
        Evidence::Met
    } else {
        Evidence::Unmet(format!("not placed yet: {}", missing.join(", ")))
    }
}

/// The client's identity is in effect at the revision being shown.
///
/// `drifted` counts: the pass that reported it corrected the drift.
/// `handEdited` does not: the provider holds something other than what was
/// declared — a client switched off, a declared callback removed — and the
/// product has deliberately not overwritten it (D01-19b). Nor does
/// `withheld`: a projected client whose stored entry is not its projection
/// was not converged at all.
///
/// Nor is identity in effect while the stored projection is not consistent,
/// whatever the status says about the entries that are declared: a missing
/// projected entry withholds nothing and can leave the rest `applied`, and a
/// projection that cannot be computed shows nothing at all. Neither is the
/// projection in effect, even with nothing withheld.
pub(crate) fn identity(status: ReconciliationStatus, projection: &ProjectionCheck) -> Evidence {
    match projection {
        ProjectionCheck::Consistent => {}
        ProjectionCheck::Diverged(_) => {
            return unmet("a stored projected identity client does not match its projection");
        }
        ProjectionCheck::Unverifiable { .. } => {
            return unmet("the stored projected identity clients cannot be checked against their projection");
        }
    }
    match status {
        ReconciliationStatus::Applied | ReconciliationStatus::Drifted => Evidence::Met,
        ReconciliationStatus::HandEdited => {
            unmet("the identity provider holds hand edits the product does not support")
        }
        ReconciliationStatus::Withheld => {
            unmet("a projected identity client was withheld: its stored entry is not its projection")
        }
        ReconciliationStatus::Pending => unmet("identity has not been applied at this revision yet"),
        ReconciliationStatus::Failed => unmet("identity could not be applied at this revision"),
    }
}

/// The client's tenant is in the tenants document the publication target
/// last accepted, at the revision its placements add up to (ADR 0023 part
/// 4, D1).
pub(crate) fn publication(client: &Client, facts: &PlatformFacts) -> Evidence {
    let placements = match &facts.placements {
        Placements::NotManaged => return unmet("this deployment publishes no runtime documents"),
        Placements::Unreadable => return unmet("the environment's placements could not be read"),
        Placements::Read(placements) => placements,
    };
    let published = match &facts.publication {
        Publication::NotConfigured => return unmet("this deployment publishes no runtime documents"),
        Publication::Observed { published: None, .. } => {
            return unmet("no publication pass has published since the control plane started")
        }
        Publication::Observed {
            published: Some(published),
            ..
        } => &published.tenants,
        Publication::Offered(tenants) => tenants,
    };
    let Ok(tenant) = TenantId::try_new(client.id.as_str()) else {
        return unmet("the client id is not a valid tenant id");
    };
    let Some(held) = published.get(&tenant) else {
        return unmet("the tenant is not in the published tenants document");
    };
    let expected: u64 = placements
        .iter()
        .filter(|record| record.tenant == tenant)
        .map(|record| record.revision.get())
        .fold(0, u64::saturating_add);

    if held.get() == expected {
        Evidence::Met
    } else {
        Evidence::Unmet(format!(
            "published at revision {}, and its placements add up to {expected}",
            held.get()
        ))
    }
}

fn unmet(detail: &str) -> Evidence {
    Evidence::Unmet(detail.to_owned())
}
