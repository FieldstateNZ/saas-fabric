//! The readiness state machine, over both of its inputs: what the registries
//! hold, and what the connector sweep found.

use super::readiness_facts::{ConnectorFacts, RegistryFacts};
use super::readiness_state::{is_degraded, is_ready};

/// A primed registry holding `count` resources.
const fn holding(count: usize) -> RegistryFacts {
    RegistryFacts { primed: true, count }
}

/// A registry that has never loaded.
const fn unprimed() -> RegistryFacts {
    RegistryFacts {
        primed: false,
        count: 0,
    }
}

/// A sweep in which every connector answered, `healthy` of them well.
const fn answered(total: usize, healthy: usize) -> ConnectorFacts {
    ConnectorFacts {
        total,
        healthy,
        unknown: 0,
    }
}

#[test]
fn unprimed_registries_are_never_ready_even_with_every_connector_healthy() {
    assert!(!is_ready(&unprimed(), &holding(4), &answered(3, 3)));
    assert!(!is_ready(&holding(3), &unprimed(), &answered(3, 3)));
    assert!(!is_ready(&unprimed(), &unprimed(), &answered(3, 3)));
}

#[test]
fn tenants_primed_over_an_empty_data_source_registry_is_not_ready() {
    // The defect this case exists for: both registries loaded, so priming says
    // yes, while every `resolve_data_source` returns `MissingDataSource` — a
    // non-retryable 500 on 100% of requests, from a replica reporting 200.
    assert!(!is_ready(&holding(3), &holding(0), &answered(1, 1)));
}

#[test]
fn a_deployment_with_nothing_onboarded_is_ready() {
    // No tenants and no DataSources is the honest empty case. Every request is
    // a truthful 404, and a brand-new deployment must be able to start.
    assert!(is_ready(&holding(0), &holding(0), &answered(1, 1)));
}

#[test]
fn data_sources_ahead_of_the_first_tenant_are_ready() {
    // Infrastructure reconciled before anybody is onboarded. Nothing is
    // broken; there is nobody to serve yet.
    assert!(is_ready(&holding(0), &holding(4), &answered(1, 1)));
}

#[test]
fn primed_with_every_connector_healthy_is_ready_and_not_degraded() {
    assert!(is_ready(&holding(3), &holding(4), &answered(3, 3)));
    assert!(!is_degraded(&answered(3, 3)));
}

#[test]
fn primed_with_some_connectors_down_is_still_ready_but_reported_degraded() {
    // The core policy decision of §34/§35: partial connector failure does not
    // pull an otherwise-useful replica out of rotation.
    assert!(is_ready(&holding(3), &holding(4), &answered(3, 1)));
    assert!(is_degraded(&answered(3, 1)));
}

#[test]
fn primed_with_every_connector_down_is_not_ready() {
    // Zero working connectors leaves this replica able to serve nothing, the
    // same situation an unprimed registry describes.
    assert!(!is_ready(&holding(3), &holding(4), &answered(3, 0)));
    assert!(is_degraded(&answered(3, 0)));
}

#[test]
fn a_connector_that_did_not_answer_in_time_keeps_the_replica_ready() {
    // Unknown is not down. Counting an unfinished check as a failure would let
    // one slow backend pull every replica, since configuration is identical
    // across them — the outcome §35 exists to prevent.
    let slow = ConnectorFacts {
        total: 1,
        healthy: 0,
        unknown: 1,
    };

    assert!(is_ready(&holding(3), &holding(4), &slow));
    assert!(is_degraded(&slow), "unknown still needs an operator's attention");
}

#[test]
fn one_unknown_among_confirmed_failures_is_enough_to_stay_ready() {
    let mostly_dead = ConnectorFacts {
        total: 3,
        healthy: 0,
        unknown: 1,
    };

    assert!(is_ready(&holding(3), &holding(4), &mostly_dead));
}

#[test]
fn no_connectors_configured_is_a_defensive_edge_case_that_still_reads_as_ready() {
    // Configuration validation always requires at least one connector, so this
    // never happens in practice — covered so the formula's behaviour at the
    // boundary is a decision, not an accident.
    assert!(is_ready(&holding(3), &holding(4), &answered(0, 0)));
    assert!(!is_degraded(&answered(0, 0)));
}
