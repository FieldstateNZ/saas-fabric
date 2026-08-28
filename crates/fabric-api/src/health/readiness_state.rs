//! The readiness decision, isolated from the I/O that feeds it.
//!
//! Pure functions over [`RegistryFacts`] and [`ConnectorFacts`], so the state
//! machine is testable without a runtime, a registry, or a connector anywhere
//! in sight.

use crate::health::readiness_facts::{ConnectorFacts, RegistryFacts};

/// Whether this replica should report itself ready.
///
/// The conjunction of two independent questions: can the reconciled state
/// serve anything, and can the connectors execute anything. Each is decided
/// on its own below.
#[must_use]
pub(super) const fn is_ready(
    tenants: &RegistryFacts,
    data_sources: &RegistryFacts,
    connectors: &ConnectorFacts,
) -> bool {
    registries_can_serve(tenants, data_sources) && connectors_can_serve(connectors)
}

/// Whether the reconciled state this replica holds can serve a request.
///
/// # Priming is necessary and is not sufficient (§28, §34)
///
/// Unprimed is never ready: with no snapshot at all, every lookup is
/// `RuntimeUnavailable` and no tenant can be served no matter how many
/// connectors are healthy.
///
/// Primed is not enough either, and this is the half that used to be missing.
/// A source publishing an honestly empty set primes a registry *empty* — the
/// load succeeded, so `fail_fast_on_prime` never fires. Two independently
/// reconciled files can therefore land out of step at startup, or a truncated
/// file can empty an already-primed registry on refresh, and the replica ends
/// up holding tenants and no DataSources. Every `resolve_data_source` then
/// returns `MissingDataSource`, which is a 500 `internal` and non-retryable:
/// a replica that fails 100% of requests, in rotation, reporting 200.
///
/// So the rule is asymmetric, and deliberately so:
///
/// | Tenants | DataSources | Verdict | Why |
/// |---|---|---|---|
/// | 0 | 0 | **ready** | A deployment with nothing onboarded must be able to start. Every request is a truthful 404. |
/// | 0 | some | **ready** | Infrastructure reconciled ahead of the first tenant. Nothing is broken; there is nobody to serve yet. |
/// | some | 0 | **not ready** | Every request 500s. This is the case that was answering 200. |
/// | some | some | **ready** | The normal case. |
///
/// Note what is *not* checked: whether each tenant's bound DataSource is
/// actually present. That is a per-tenant fact, and one tenant's broken
/// binding must not pull the replica serving the other 199 — the same
/// reasoning that keeps partial connector failure out of the verdict below.
const fn registries_can_serve(tenants: &RegistryFacts, data_sources: &RegistryFacts) -> bool {
    tenants.primed && data_sources.primed && (tenants.count == 0 || data_sources.count > 0)
}

/// Whether the connectors leave this replica able to execute anything.
///
/// # Partial failure is tolerated (§35)
///
/// Readiness does **not** require every connector to be healthy. A replica
/// serving 9 of 10 connectors is still doing useful work, and marking it
/// unready pulls it out of rotation for every tenant, including the ones on
/// the 9 that are fine. Configuration is identical across replicas, so if one
/// replica has a connector down, every replica does; flipping all of them
/// unready would not free up a single working replica — it would remove 100%
/// of capacity to protest 10% of it.
///
/// # An unfinished check is not a failed one
///
/// A connector whose check had not answered when the sweep's budget expired
/// counts towards readiness exactly as a healthy one does. It is *unknown*,
/// not *down*, and the argument above applies with more force: treating
/// absence of evidence as evidence of failure would let one slow backend do
/// precisely what §35 says it must not. It still shows up as degraded, and an
/// authorised caller sees it named as `unknown`, so nothing is hidden.
///
/// Readiness fails only when **every** configured connector has definitively
/// answered that it is unhealthy — a replica with nowhere left to execute is
/// in the same position as one holding no reconciled state at all.
const fn connectors_can_serve(connectors: &ConnectorFacts) -> bool {
    connectors.total == 0 || connectors.healthy > 0 || connectors.unknown > 0
}

/// Whether some, but not all, connectors are confirmed healthy.
///
/// Surfaced separately from [`is_ready`] so an operator can tell "up and fully
/// healthy" apart from "up, but something needs attention" without scanning
/// the per-connector list by hand. Unknown counts as degraded: a check that
/// could not answer inside half a second is itself worth looking at.
#[must_use]
pub(super) const fn is_degraded(connectors: &ConnectorFacts) -> bool {
    connectors.healthy < connectors.total
}
