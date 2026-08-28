//! Tests for what a pass is called, given what came before.

use crate::fixtures::revision;
use crate::status::transition::status_for;
use crate::status::{ReconciliationReport, ReconciliationStatus};
use crate::{ProviderError, ReconciliationOutcome};

/// A previously recorded report.
fn recorded(status: ReconciliationStatus, at_revision: &str) -> ReconciliationReport {
    ReconciliationReport {
        status,
        revision: revision(at_revision),
        actions: 0,
        observed_at_unix: 0,
        detail: None,
    }
}

#[test]
fn a_failed_pass_is_failed_whatever_came_before() {
    let outcome = ReconciliationOutcome::failed(&ProviderError::NotPermitted);
    let previous = recorded(ReconciliationStatus::Applied, "abc");

    assert_eq!(
        status_for(Some(&previous), &revision("abc"), &outcome),
        ReconciliationStatus::Failed
    );
}

#[test]
fn a_pass_that_changed_nothing_is_applied() {
    let outcome = ReconciliationOutcome::converged();

    assert_eq!(
        status_for(None, &revision("abc"), &outcome),
        ReconciliationStatus::Applied
    );
}

#[test]
fn first_convergence_of_a_new_client_is_applied_not_drifted() {
    let outcome = ReconciliationOutcome::applied(4);

    assert_eq!(
        status_for(None, &revision("abc"), &outcome),
        ReconciliationStatus::Applied
    );
}

#[test]
fn converging_a_newly_written_revision_is_applied() {
    // The operator changed the document; the provider was correct for the old
    // one. Correcting it is the platform doing its job, not drift.
    let outcome = ReconciliationOutcome::applied(1);
    let previous = recorded(ReconciliationStatus::Applied, "abc");

    assert_eq!(
        status_for(Some(&previous), &revision("def"), &outcome),
        ReconciliationStatus::Applied
    );
}

#[test]
fn converging_after_a_pending_write_is_applied() {
    let outcome = ReconciliationOutcome::applied(1);
    let previous = recorded(ReconciliationStatus::Pending, "def");

    assert_eq!(
        status_for(Some(&previous), &revision("def"), &outcome),
        ReconciliationStatus::Applied
    );
}

#[test]
fn correcting_an_already_converged_client_is_drift() {
    // Nothing in Git changed and the provider still needed correcting, so
    // something outside SaaS Fabric edited a realm the platform owns. That is
    // worth saying out loud rather than silently fixing.
    let outcome = ReconciliationOutcome::applied(1);
    let previous = recorded(ReconciliationStatus::Applied, "abc");

    assert_eq!(
        status_for(Some(&previous), &revision("abc"), &outcome),
        ReconciliationStatus::Drifted
    );
}

#[test]
fn a_recovered_failure_is_applied_rather_than_drifted() {
    let outcome = ReconciliationOutcome::applied(2);
    let previous = recorded(ReconciliationStatus::Failed, "abc");

    assert_eq!(
        status_for(Some(&previous), &revision("abc"), &outcome),
        ReconciliationStatus::Applied
    );
}
