//! The readiness decision, isolated from the I/O that feeds it.
//!
//! Kept as pure functions over plain counts so the state machine — unprimed,
//! primed-and-healthy, primed-and-degraded, primed-and-dead — is testable
//! without a runtime, a registry, or a connector anywhere in sight.

/// Whether this replica should report itself ready.
///
/// # The decision (§34, §35)
///
/// Unprimed registries are always not-ready: with no tenant bindings or no
/// DataSources loaded, this replica can serve no tenant request no matter how
/// many connectors are healthy.
///
/// Once primed, readiness does **not** require every connector to be
/// healthy. §35 tolerates partial connector failure by design — a replica
/// serving 9 of 10 connectors is still doing useful work, and marking it
/// unready pulls it out of rotation for every tenant, including the ones on
/// the 9 connectors that are fine. Configuration is identical across
/// replicas, so if one replica has a connector down, every replica does;
/// flipping all of them unready would not free up a single working replica —
/// it would remove 100% of capacity to protest 10% of it.
///
/// Readiness fails only when **every** configured connector is unhealthy,
/// because a replica with zero working connectors is in exactly the
/// situation an unprimed registry describes: it can serve nothing, and an
/// orchestrator should stop sending it traffic.
#[must_use]
pub(super) const fn is_ready(
    tenants_primed: bool,
    data_sources_primed: bool,
    total_connectors: usize,
    healthy_connectors: usize,
) -> bool {
    tenants_primed && data_sources_primed && (total_connectors == 0 || healthy_connectors > 0)
}

/// Whether some, but not all, connectors are unhealthy.
///
/// Surfaced separately from [`is_ready`] so an operator can tell "up and
/// fully healthy" apart from "up, but something needs attention" without
/// scanning the per-connector list by hand.
#[must_use]
pub(super) const fn is_degraded(total_connectors: usize, healthy_connectors: usize) -> bool {
    healthy_connectors < total_connectors
}
