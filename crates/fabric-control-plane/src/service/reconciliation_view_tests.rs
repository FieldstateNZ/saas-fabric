//! Tests for what an operator is told about reconciliation.

use fabric_client_model::ClientRevision;
use fabric_reconciliation::{ReconciliationReport, ReconciliationStatus};

use super::reconciliation_view::resolve;

fn revision(value: &str) -> ClientRevision {
    ClientRevision::try_new(value).unwrap()
}

fn report(status: ReconciliationStatus, at: &str) -> ReconciliationReport {
    ReconciliationReport {
        status,
        revision: revision(at),
        actions: 1,
        observed_at_unix: 1_700_000_000,
        detail: Some("the identity provider is unavailable: refused".to_owned()),
    }
}

#[test]
fn a_client_never_reconciled_is_pending() {
    let view = resolve(None, &revision("abc"));

    assert_eq!(view.status, ReconciliationStatus::Pending);
    assert_eq!(view.observed_at_unix, None);
    assert_eq!(view.detail, None);
}

#[test]
fn a_report_for_this_revision_is_shown_as_it_is() {
    let view = resolve(
        Some(&report(ReconciliationStatus::Applied, "abc")),
        &revision("abc"),
    );

    assert_eq!(view.status, ReconciliationStatus::Applied);
    assert_eq!(view.observed_at_unix, Some(1_700_000_000));
}

#[test]
fn a_report_for_an_older_revision_is_pending_again() {
    // The green-tick bug this closes: the operator wrote a new revision, so
    // "applied" is a statement about something that is no longer wanted.
    let view = resolve(
        Some(&report(ReconciliationStatus::Applied, "abc")),
        &revision("def"),
    );

    assert_eq!(view.status, ReconciliationStatus::Pending);
}

#[test]
fn a_stale_failure_detail_is_not_shown_beside_fresh_state() {
    let view = resolve(
        Some(&report(ReconciliationStatus::Failed, "abc")),
        &revision("def"),
    );

    assert_eq!(view.status, ReconciliationStatus::Pending);
    assert_eq!(
        view.detail, None,
        "last revision's error must not read as current"
    );
}

#[test]
fn a_failure_for_this_revision_keeps_its_detail() {
    let view = resolve(
        Some(&report(ReconciliationStatus::Failed, "abc")),
        &revision("abc"),
    );

    assert_eq!(view.status, ReconciliationStatus::Failed);
    assert!(view.detail.is_some());
}

#[test]
fn drift_for_this_revision_is_reported_as_drift() {
    let view = resolve(
        Some(&report(ReconciliationStatus::Drifted, "abc")),
        &revision("abc"),
    );

    assert_eq!(view.status, ReconciliationStatus::Drifted);
}
