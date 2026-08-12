//! The readiness state machine: unprimed, primed+healthy, primed+partial,
//! primed+none healthy.

use super::readiness_state::{is_degraded, is_ready};

#[test]
fn unprimed_registries_are_never_ready_even_with_every_connector_healthy() {
    assert!(!is_ready(false, true, 3, 3));
    assert!(!is_ready(true, false, 3, 3));
    assert!(!is_ready(false, false, 3, 3));
}

#[test]
fn primed_with_every_connector_healthy_is_ready_and_not_degraded() {
    assert!(is_ready(true, true, 3, 3));
    assert!(!is_degraded(3, 3));
}

#[test]
fn primed_with_some_connectors_down_is_still_ready_but_reported_degraded() {
    // The core policy decision of §34/§35: partial connector failure does not
    // pull an otherwise-useful replica out of rotation.
    assert!(is_ready(true, true, 3, 1));
    assert!(is_degraded(3, 1));
}

#[test]
fn primed_with_every_connector_down_is_not_ready() {
    // Zero working connectors leaves this replica able to serve nothing, the
    // same situation an unprimed registry describes.
    assert!(!is_ready(true, true, 3, 0));
    assert!(is_degraded(3, 0));
}

#[test]
fn no_connectors_configured_is_a_defensive_edge_case_that_still_reads_as_ready() {
    // Configuration validation always requires at least one connector, so
    // this never happens in practice — covered so the formula's behaviour at
    // the boundary is a decision, not an accident.
    assert!(is_ready(true, true, 0, 0));
    assert!(!is_degraded(0, 0));
}
