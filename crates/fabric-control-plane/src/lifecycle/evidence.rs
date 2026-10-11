//! The evidence for each readiness condition, from facts the control plane
//! already holds (D01-19a). Pure, so each rule is a unit test.
//!
//! Absence of evidence is never readiness: a platform this deployment does
//! not manage, a placements document that could not be read, or a process
//! that has not completed a publication pass since it started all leave the
//! condition unmet, with the reason.

use fabric_client_model::lifecycle::Evidence;
use fabric_client_model::Client;
use fabric_core::TenantId;
use fabric_platform_management::{PlacementRecord, PublishedTenants};
use fabric_reconciliation::ReconciliationStatus;

/// What the control plane could learn about the managed platform.
pub(crate) enum PlatformFacts {
    /// This deployment manages no platform.
    NotManaged,
    /// The environment's placements could not be read.
    Unreadable,
    /// The placements, and what is known to be published.
    Read {
        /// Every recorded placement in the environment.
        placements: Vec<PlacementRecord>,
        /// What the last successful publication pass left published.
        published: Published,
    },
}

/// What is known to be published.
pub(crate) enum Published {
    /// This deployment has no publication target.
    NoPublisher,
    /// No pass has published or found nothing to change since this process
    /// started.
    NotYet,
    /// The tenants a pass left published.
    Known(PublishedTenants),
}

/// Every logical data source the client declares has a placement.
pub(crate) fn provisioning(client: &Client, facts: &PlatformFacts) -> Evidence {
    let placements = match facts {
        PlatformFacts::NotManaged => return unmet("this deployment manages no platform to place data on"),
        PlatformFacts::Unreadable => return unmet("the environment's placements could not be read"),
        PlatformFacts::Read { placements, .. } => placements,
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
pub(crate) fn identity(status: ReconciliationStatus) -> Evidence {
    match status {
        ReconciliationStatus::Applied | ReconciliationStatus::Drifted => Evidence::Met,
        ReconciliationStatus::Pending => unmet("identity has not been applied at this revision yet"),
        ReconciliationStatus::Failed => unmet("identity could not be applied at this revision"),
    }
}

/// The client's tenant is in the published tenants document, at the revision
/// its placements add up to (ADR 0023 part 4, D1).
pub(crate) fn publication(client: &Client, facts: &PlatformFacts) -> Evidence {
    let (placements, published) = match facts {
        PlatformFacts::NotManaged => return unmet("this deployment publishes no runtime documents"),
        PlatformFacts::Unreadable => return unmet("the environment's placements could not be read"),
        PlatformFacts::Read {
            placements,
            published,
        } => (placements, published),
    };
    let published = match published {
        Published::NoPublisher => return unmet("this deployment publishes no runtime documents"),
        Published::NotYet => {
            return unmet("no publication pass has completed since the control plane started")
        }
        Published::Known(published) => published,
    };
    let Ok(tenant) = TenantId::try_new(client.id.as_str()) else {
        return unmet("the client id is not a valid tenant id");
    };
    let Some(held) = published.tenants.get(&tenant) else {
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
