//! Whether a DataSource can actually provide the isolation a binding asks
//! for.
//!
//! Its own module rather than a method on the resolver, because it is a
//! different question. Resolution walks a chain and answers *which*
//! DataSource; this answers whether the one it found may be used at all. The
//! two failing for unrelated reasons is easier to read when they are not
//! interleaved.

use fabric_connector::IsolationModel;
use fabric_core::TenantId;

use crate::data_source::PlacementClass;
use crate::resolution::destination_exclusivity::Exclusivity;
use crate::{DataSource, ResolveError};

/// Refuses a binding whose isolation the DataSource cannot actually provide.
///
/// The one place placement is read on the request path, and it is read to
/// *refuse*, never to choose — see
/// [`placement_inertness_tests`](crate::resolution) for the tests that pin
/// the difference. Nothing here can change which DataSource is resolved;
/// it can only decide that the resolved one must not be used.
///
/// # Two rules, because a label is a claim and a count is a fact
///
/// [ADR 0006] refused structural isolation on
/// [`PlacementClass::Shared`](crate::PlacementClass::Shared). That rule was
/// keyed on a *label*, and there are six placement classes: four of the other
/// five — `HighAvailability`, `Regulated`, `Development`, `Ephemeral` — assert
/// nothing about single tenancy, and a clustered Postgres or a dev sandbox
/// serving many tenants is exactly what they describe. The shipped example
/// already had one: `initech-dedicated`, `placement: regulated`,
/// `accepts_new_tenants: true`, structurally identical to the configuration
/// the ADR was written about while wearing a label the rule never inspected.
///
/// Narrowing the allowlist to `Dedicated` would move the hole, not close it,
/// because `Dedicated` is a claim too — nothing stops reconciliation binding
/// a second tenant to one. So there is a second rule, keyed on what the
/// runtime can *observe*: [`Exclusivity`], derived from the snapshots
/// themselves. Either rule alone refuses.
///
/// The label rule is kept rather than replaced. `Shared` with one tenant on it
/// is an operator saying "more will be placed here" — usually with
/// `accepts_new_tenants: true` — and refusing that before the second tenant
/// arrives is better than refusing it after.
///
/// # What this still does not catch
///
/// [`Exclusivity`] sets out the limits in full; the short version is that the
/// runtime compares *configuration*, never infrastructure. Two differently
/// named connections reaching one database, or two secret references a store
/// aliases onto one credential, are indistinguishable from two databases here,
/// and asking a connector would put a control-plane round trip on the request
/// path that §6 forbids. Structural isolation therefore still rests on the
/// operator wiring distinct connections to distinct places; what has changed is
/// that every way of getting that wrong *within configuration* is now refused
/// rather than served silently — including two secret references that differ
/// only in case or punctuation, which a resolver's own projection can flatten
/// into one credential without anything reaching for the network to notice.
///
/// # Errors
///
/// [`ResolveError::IsolationNotEnforceable`], whose docs carry the full
/// reasoning: a shared DataSource has one connection, and structural
/// isolation has no predicate, so the two together isolate nothing.
///
/// [ADR 0006]: https://github.com/brettsmith/saas-fabric/blob/main/docs/decisions/0006-a-shared-data-source-can-only-serve-discriminator-isolation.md
pub(super) fn check_isolation_is_enforceable(
    tenant: &TenantId,
    data_source: &DataSource,
    isolation: &IsolationModel,
    exclusivity: Exclusivity,
) -> Result<(), ResolveError> {
    // Discriminator isolation carries its own predicate, so it stays safe on
    // every placement and however many tenants share the destination. That is
    // the model shared placement exists to serve, and nothing below may
    // narrow it.
    if !matches!(
        isolation,
        IsolationModel::Database | IsolationModel::Schema { .. }
    ) {
        return Ok(());
    }

    let declared_shared = data_source.placement == PlacementClass::Shared;

    if declared_shared || exclusivity == Exclusivity::Shared {
        return Err(ResolveError::IsolationNotEnforceable {
            tenant: tenant.clone(),
            data_source: data_source.id.clone(),
            isolation: isolation.telemetry_label(),
        });
    }

    Ok(())
}
