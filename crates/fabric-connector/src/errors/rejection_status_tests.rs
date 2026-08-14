//! What each rejection status is allowed to claim.
//!
//! Every status the NDC specification defines is named here individually, and
//! deliberately not as a range. The rule these pin is not "4xx did not apply" —
//! that rule is wrong, and `rejection_status`'s docs argue why — so a test
//! written as `(400..500)` would quietly re-assert the broad version the moment
//! somebody widened the match to agree with it.

use crate::errors::rejection_effect;
use crate::OperationEffect;

#[test]
fn a_request_the_connector_would_not_accept_certainly_did_not_apply() {
    // 400 and 422 are defined as refusals of the request itself: a shape the
    // connector did not expect, and a value of the wrong type. Neither is
    // reachable after the data source has been touched.
    assert_eq!(rejection_effect(400), OperationEffect::NotApplied);
    assert_eq!(rejection_effect(422), OperationEffect::NotApplied);
}

#[test]
fn a_constraint_failure_may_still_have_written_rows() {
    // The load-bearing case. 403 and 409 are 4xx, and the specification's own
    // examples for them -- a check constraint, a foreign key constraint -- are
    // failures raised *by the data source during the write*. Nothing in the
    // specification makes a single opaque procedure atomic, so one that wrote
    // before it tripped the constraint is conformant. Claiming `NotApplied`
    // here would tell a caller their records are absent when they are present.
    assert_eq!(rejection_effect(403), OperationEffect::Unknown);
    assert_eq!(rejection_effect(409), OperationEffect::Unknown);
}

#[test]
fn a_server_side_failure_claims_nothing() {
    for status in [500, 501, 502] {
        assert_eq!(rejection_effect(status), OperationEffect::Unknown, "{status}");
    }
}

#[test]
fn a_status_the_specification_does_not_define_claims_nothing() {
    // These reach us from proxies, sidecars and gateways the connector never
    // saw. A 408 in particular is 4xx and genuinely ambiguous: it says a
    // complete request did not arrive somewhere, not that nothing ran.
    for status in [401, 404, 408, 413, 429] {
        assert_eq!(rejection_effect(status), OperationEffect::Unknown, "{status}");
    }
}

#[test]
fn a_success_status_is_never_asked_but_still_claims_nothing() {
    // `rejection_effect` is only reached from a `Rejected`, which is only built
    // for a non-success status. Pinned anyway so the fallback arm cannot become
    // an accidental `NotApplied` for the one input that would be a lie.
    assert_eq!(rejection_effect(200), OperationEffect::Unknown);
}
