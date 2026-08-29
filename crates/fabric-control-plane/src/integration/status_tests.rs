//! What the platform reports about its connection, and why.

use super::*;
use crate::repository::RepositoryError;

/// A health record that has seen one thing.
fn having_seen(observation: Observation) -> IntegrationHealth {
    let health = IntegrationHealth::new();
    health.record(observation, 1_000);
    health
}

#[test]
fn nothing_connected_is_reported_as_not_configured() {
    let health = IntegrationHealth::new();

    assert_eq!(health.status(false), IntegrationStatus::NotConfigured);
}

#[test]
fn a_binding_with_no_sweep_yet_is_connected_rather_than_broken() {
    // A restart must not show a fault for the seconds before the first sweep.
    let health = IntegrationHealth::new();

    assert_eq!(health.status(true), IntegrationStatus::Connected);
}

#[test]
fn a_successful_read_is_connected() {
    assert_eq!(
        having_seen(Observation::Read).status(true),
        IntegrationStatus::Connected
    );
}

#[test]
fn a_refused_credential_is_invalid_so_the_console_can_offer_a_reconnect() {
    assert_eq!(
        having_seen(Observation::Refused).status(true),
        IntegrationStatus::Invalid
    );
}

#[test]
fn any_other_failure_is_an_error_rather_than_an_invitation_to_reconnect() {
    assert_eq!(
        having_seen(Observation::Failed).status(true),
        IntegrationStatus::Error
    );
}

#[test]
fn only_a_refused_credential_classifies_as_refused() {
    assert_eq!(
        Observation::of(&RepositoryError::NotPermitted),
        Observation::Refused
    );

    for error in [
        RepositoryError::Unavailable {
            detail: "connection reset".to_owned(),
        },
        RepositoryError::Rejected {
            detail: "branch is protected".to_owned(),
        },
        RepositoryError::Conflict,
    ] {
        assert_eq!(
            Observation::of(&error),
            Observation::Failed,
            "{error} should not invite a reconnect"
        );
    }

    assert_eq!(
        Observation::of(&RepositoryError::NotConfigured),
        Observation::NotConfigured
    );
}

#[test]
fn a_failing_integration_still_reports_when_it_last_worked() {
    let health = IntegrationHealth::new();
    health.record(Observation::Read, 1_000);
    health.record(Observation::Refused, 2_000);

    assert_eq!(health.status(true), IntegrationStatus::Invalid);
    assert_eq!(
        health.last_success(),
        Some(1_000),
        "the last good read is the first thing asked about a broken integration"
    );
}

#[test]
fn a_platform_that_has_never_read_reports_no_last_success() {
    assert_eq!(having_seen(Observation::Failed).last_success(), None);
}

#[test]
fn an_unconfigured_platform_reports_not_configured_whatever_it_last_saw() {
    // Disconnecting unbinds; the stale observation must not outrank that.
    assert_eq!(
        having_seen(Observation::Read).status(false),
        IntegrationStatus::NotConfigured
    );
}
