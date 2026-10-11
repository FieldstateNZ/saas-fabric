//! A retired client's withdrawal is reported from publication evidence
//! only, and never as done.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};
use fabric_platform_management::{LastPass, PassOutcome, PublishedTenants, SafeDiagnostic, WaitingReason};

use super::facts::Publication;
use super::withdrawal::{withdrawal, WithdrawalState};

fn acme() -> TenantId {
    TenantId::try_new("acme").unwrap()
}

fn published(tenants: &[&str]) -> PublishedTenants {
    PublishedTenants {
        at_unix_seconds: 1,
        tenants: tenants
            .iter()
            .map(|tenant| (TenantId::try_new(*tenant).unwrap(), BindingRevision::new(2)))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn last(outcome: PassOutcome) -> LastPass {
    LastPass {
        at_unix_seconds: 2,
        outcome,
    }
}

fn state(publication: &Publication) -> WithdrawalState {
    withdrawal(Some(&acme()), publication).state
}

#[test]
fn a_tenant_still_in_the_accepted_document_is_pending() {
    let report = withdrawal(
        Some(&acme()),
        &Publication::Observed {
            last_pass: None,
            published: Some(published(&["acme", "globex"])),
        },
    );

    assert_eq!(report.state, WithdrawalState::Pending);
    assert!(
        report.detail.contains("still carries this tenant, at revision 2"),
        "{}",
        report.detail
    );
}

#[test]
fn a_refused_or_failed_last_pass_is_reported_as_refused_even_if_the_tenant_was_published() {
    for outcome in [
        PassOutcome::Refused {
            reason: SafeDiagnostic::sanitise("would empty the tenants document"),
        },
        PassOutcome::Failed {
            detail: SafeDiagnostic::sanitise("the target could not be reached"),
        },
    ] {
        let report = withdrawal(
            Some(&acme()),
            &Publication::Observed {
                last_pass: Some(last(outcome)),
                published: Some(published(&["acme"])),
            },
        );

        assert_eq!(report.state, WithdrawalState::Refused);
        assert!(
            report.detail.contains("keeps the documents it last accepted"),
            "{}",
            report.detail
        );
    }
}

#[test]
fn no_publisher_or_no_pass_since_start_is_unobserved() {
    assert_eq!(state(&Publication::NotConfigured), WithdrawalState::Unobserved);
    assert_eq!(
        state(&Publication::Observed {
            last_pass: Some(last(PassOutcome::Waiting {
                reason: WaitingReason::NoResources,
            })),
            published: None,
        }),
        WithdrawalState::Unobserved
    );
}

#[test]
fn a_tenant_the_accepted_document_does_not_carry_is_not_published_and_never_withdrawn() {
    let report = withdrawal(
        Some(&acme()),
        &Publication::Observed {
            last_pass: None,
            published: Some(published(&["globex"])),
        },
    );

    assert_eq!(report.state, WithdrawalState::NotPublished);
    assert!(
        report.detail.contains("no reader is observed"),
        "{}",
        report.detail
    );
}
