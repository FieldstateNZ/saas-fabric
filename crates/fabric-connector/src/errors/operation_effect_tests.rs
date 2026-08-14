//! What each failure claims about whether the operation took effect.
//!
//! These read as restatements of the `match` they cover, and that is on
//! purpose. The claim each variant makes is the contract the Data API maps to a
//! status code, so a change to one of these answers should have to be typed out
//! twice — once in the code and once here — rather than slipping through as a
//! newly-added arm somebody grouped with its neighbours.

use crate::{CollectionName, ConnectorError, ConnectorId, OperationEffect, UnsupportedFeature};

fn connector() -> ConnectorId {
    ConnectorId::try_new("postgres").unwrap()
}

fn transport_cause() -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset"))
}

#[test]
fn an_unreachable_connector_certainly_did_not_apply_the_write() {
    // The narrow variant, and the only transport failure a retry is safe on.
    let error = ConnectorError::Unreachable {
        connector: connector(),
        source: transport_cause(),
    };

    assert_eq!(error.effect(), OperationEffect::NotApplied);
}

#[test]
fn a_request_that_went_out_unanswered_may_have_applied_the_write() {
    let error = ConnectorError::OutcomeUnknown {
        connector: connector(),
        source: transport_cause(),
    };

    assert_eq!(error.effect(), OperationEffect::Unknown);
}

#[test]
fn a_lost_response_after_a_success_status_certainly_did_apply_the_write() {
    let error = ConnectorError::ResultLost {
        connector: connector(),
        source: transport_cause(),
    };

    assert_eq!(error.effect(), OperationEffect::Applied);
}

#[test]
fn a_malformed_response_also_means_the_write_applied() {
    // Only ever built after a success status — `decode_body` returns `Rejected`
    // for anything else. So the backend ran the operation and said it worked;
    // what it said afterwards was unreadable.
    let error = ConnectorError::MalformedResponse {
        connector: connector(),
        detail: "missing field `operation_results`".to_owned(),
    };

    assert_eq!(error.effect(), OperationEffect::Applied);
}

#[test]
fn a_rejection_claims_no_more_than_unknown() {
    // A 4xx did not apply and a 5xx may have; the status is not carried here to
    // tell them apart. Claiming `NotApplied` would be a guess in the direction
    // that loses data.
    let error = ConnectorError::Rejected {
        connector: connector(),
        message: "relation does not exist".to_owned(),
    };

    assert_eq!(error.effect(), OperationEffect::Unknown);
}

#[test]
fn every_locally_refused_failure_certainly_did_not_apply() {
    let refused = [
        ConnectorError::UnknownConnector(connector()),
        ConnectorError::UnknownCollection(CollectionName::try_new("customers").unwrap()),
        ConnectorError::SecretUnavailable {
            reference: "tenant/acme/data-primary".to_owned(),
        },
        ConnectorError::InvalidOperation("no filter_argument".to_owned()),
        UnsupportedFeature::DeletesOnCollection.refused_because("no mapping".to_owned()),
    ];

    for error in refused {
        assert_eq!(error.effect(), OperationEffect::NotApplied, "{error:?}");
    }
}

#[test]
fn the_three_transport_variants_agree_on_fault_and_disagree_on_effect() {
    // `is_internal` and `effect` answer different questions, which is why one
    // flag could not have carried both. All three are the platform's problem;
    // only one of them is safe to retry.
    let variants = [
        ConnectorError::Unreachable {
            connector: connector(),
            source: transport_cause(),
        },
        ConnectorError::OutcomeUnknown {
            connector: connector(),
            source: transport_cause(),
        },
        ConnectorError::ResultLost {
            connector: connector(),
            source: transport_cause(),
        },
    ];

    assert!(variants.iter().all(ConnectorError::is_internal));

    let effects: Vec<_> = variants.iter().map(ConnectorError::effect).collect();
    assert_eq!(
        effects,
        [
            OperationEffect::NotApplied,
            OperationEffect::Unknown,
            OperationEffect::Applied
        ]
    );
}
