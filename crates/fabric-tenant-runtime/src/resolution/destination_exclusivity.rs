//! Whether any *other* tenant reaches a DataSource's physical destination.

use fabric_core::{DataSourceId, TenantId};

use crate::{DataSourceRegistry, TenantRegistry};

/// What the runtime has *observed* about how many tenants reach one
/// DataSource's physical destination.
///
/// # Why this is observed rather than declared
///
/// A [`PlacementClass`](crate::PlacementClass) is an operator's claim, and
/// four of the six classes claim nothing about single tenancy at all. "Which
/// tenants are bound to this DataSource" and "which other DataSources name the
/// same connection" are facts in the snapshots the runtime is already holding,
/// so no claim has to be believed for either.
///
/// # One rule, two facts, one per registry
///
/// The rule is a single sentence — *more than one tenant reaches this
/// destination* — and it is assembled from two derived facts, deliberately one
/// per registry, because the two registries refresh independently and a fact
/// spanning both would go stale every time one of them moved:
///
/// - [`CoTenancy`](crate::CoTenancy) comes from the tenant snapshot alone. A
///   binding names its DataSource, so occupancy needs no DataSource loaded.
/// - [`DestinationReuse`](crate::DestinationReuse) comes from the DataSource
///   snapshot alone. Two DataSources naming one connector and one connection
///   are one database, and no tenant is needed to see that.
///
/// They are only *combined* here, per request, which is what makes the answer
/// both O(1) and current: a refresh that introduces sharing changes the verdict
/// on the very next request, and a refresh of either registry alone cannot
/// leave the other's fact behind.
///
/// Note that reuse alone is not a refusal. One tenant holding a writable and a
/// read-only DataSource over one database shares a destination with nobody, and
/// flagging the reuse without asking who occupies the peer would refuse a
/// perfectly sound arrangement.
///
/// # What neither fact can see
///
/// Configuration only. Two [`ConnectionSelector::Named`] values reaching one
/// database, two [`SecretRef`]s resolving to one credential, or a connector
/// whose default connection is the server another named connection points at,
/// all read as distinct destinations here. Closing those would mean asking a
/// connector on the request path, which §6 forbids for the sound reason that it
/// turns a control-plane outage into a data-plane one.
///
/// [`ConnectionSelector::Named`]: fabric_connector::ConnectionSelector::Named
/// [`SecretRef`]: fabric_connector::SecretRef
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Exclusivity {
    /// No other tenant the runtime can see reaches this destination.
    Exclusive,

    /// Another tenant reaches it too — so the connection that was supposed to
    /// separate tenants does not.
    Shared,
}

/// Combines both registries' derived facts for one tenant and DataSource.
///
/// Unprimed registries report `Exclusive` because they have observed nothing.
/// That is safe rather than optimistic: resolution cannot reach this point
/// without both lookups having succeeded, and an unprimed registry answers
/// those with [`LookupError::Unavailable`](crate::LookupError) first.
pub(super) fn observe(
    tenants: &TenantRegistry,
    data_sources: &DataSourceRegistry,
    tenant: &TenantId,
    data_source: &DataSourceId,
) -> Exclusivity {
    // Both guards are held across the whole decision, so the two facts are
    // read as one consistent pair rather than either being replaced halfway.
    let shared = tenants.with_set_facts(|co_tenancy| {
        let Some(co_tenancy) = co_tenancy else {
            return false;
        };

        if co_tenancy.has_tenants_other_than(data_source, tenant) {
            return true;
        }

        data_sources.with_set_facts(|reuse| {
            reuse.is_some_and(|reuse| {
                reuse
                    .peers(data_source)
                    .iter()
                    .any(|peer| co_tenancy.has_tenants_other_than(peer, tenant))
            })
        })
    });

    if shared {
        Exclusivity::Shared
    } else {
        Exclusivity::Exclusive
    }
}
