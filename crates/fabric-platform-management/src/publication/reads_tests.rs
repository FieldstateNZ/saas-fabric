//! `classify`: which desired-state read failures wait, which are refused,
//! and which are a transport failure to retry.

use super::*;

#[test]
fn a_refused_store_is_a_human_problem_not_a_transport_one() {
    // A malformed held document, a wrong `environment:` header, or a
    // refused credential -- the store understood the request and said no.
    let error = DesiredStateError::Refused {
        detail: "environment header does not match".to_owned(),
    };

    assert!(matches!(classify(&error), Halt::Refused(_)));
}

#[test]
fn an_unconnected_platform_repository_is_a_wait_not_a_failure() {
    let halt = classify(&DesiredStateError::NotConnected);

    assert!(matches!(halt, Halt::Waiting(WaitingReason::PlatformNotConnected)));
}

#[test]
fn an_unavailable_store_is_a_transport_failure() {
    let error = DesiredStateError::Unavailable {
        detail: "github: 500 while reading environments/lucentroot/data-sources.yaml".to_owned(),
    };

    assert!(matches!(classify(&error), Halt::Failed(_)));
}

#[test]
fn a_conflict_is_also_a_transport_failure_here() {
    // Reading never races a write the way a compare-and-swap does; reaching
    // `Conflict` from a read is the adapter's own transient state, not a
    // coherence problem to refuse.
    assert!(matches!(classify(&DesiredStateError::Conflict), Halt::Failed(_)));
}
