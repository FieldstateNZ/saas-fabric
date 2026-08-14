//! Whether a failed call tells the truth about what the connector did.
//!
//! These run against [`FakeConnector`], a real socket, because the thing under
//! test is *where in the TCP exchange the failure happened* — which a mocked
//! transport cannot reproduce. Each case asserts two things together: what the
//! connector had already applied, and what the platform then said about it.
//! Asserting only the second is how the original defect passed review.

use std::collections::BTreeMap;

use fabric_connector::{ConnectorError, OperationEffect};
use serde_json::Value;

use crate::client::fake_connector::{FakeConnector, Misbehaviour};
use crate::client::NdcHttpClient;
use crate::config::NdcConnectorConfig;

fn client_for(endpoint: &str) -> NdcHttpClient {
    let mut config = NdcConnectorConfig::for_test(BTreeMap::new());
    config.endpoint = endpoint.to_owned();
    config.http_timeout_seconds = 1;
    config.http_connect_timeout_seconds = 1;

    NdcHttpClient::new(&config).unwrap()
}

/// Posts a mutation-shaped body and returns however it failed.
async fn post_mutation(endpoint: &str) -> ConnectorError {
    client_for(endpoint)
        .post::<Value, Value>("/mutation", &Value::Object(serde_json::Map::new()))
        .await
        .unwrap_err()
}

/// An endpoint with nothing listening on it.
async fn dead_endpoint() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    endpoint
}

#[tokio::test]
async fn a_body_truncated_after_ok_reports_the_write_as_applied() {
    // The connector read the whole request, committed, and answered `200`. Only
    // the body was lost. Anything but `Applied` here understates what happened.
    let fake = FakeConnector::start(Misbehaviour::TruncateBodyAfterOk).await;

    let error = post_mutation(fake.endpoint()).await;

    assert_eq!(fake.applied(), 1);
    assert!(matches!(error, ConnectorError::ResultLost { .. }), "{error:?}");
    assert_eq!(error.effect(), OperationEffect::Applied);
}

#[tokio::test]
async fn a_timeout_after_the_body_was_sent_reports_the_outcome_as_unknown() {
    // The request is on the wire and read in full; the client's total timeout
    // is what ends the call. The write may well have happened.
    let fake = FakeConnector::start(Misbehaviour::NeverAnswer).await;

    let error = post_mutation(fake.endpoint()).await;

    assert_eq!(fake.applied(), 1);
    assert!(
        matches!(error, ConnectorError::OutcomeUnknown { .. }),
        "{error:?}"
    );
    assert_eq!(error.effect(), OperationEffect::Unknown);
}

#[tokio::test]
async fn a_connection_closed_before_any_status_reports_the_outcome_as_unknown() {
    let fake = FakeConnector::start(Misbehaviour::CloseWithoutAnswering).await;

    let error = post_mutation(fake.endpoint()).await;

    assert_eq!(fake.applied(), 1);
    assert!(
        matches!(error, ConnectorError::OutcomeUnknown { .. }),
        "{error:?}"
    );
    assert_eq!(error.effect(), OperationEffect::Unknown);
}

#[tokio::test]
async fn a_refused_connect_reports_the_write_as_not_applied() {
    // The only one of the four that is safe to retry, and the only one that may
    // carry a retryable status.
    let error = post_mutation(&dead_endpoint().await).await;

    assert!(matches!(error, ConnectorError::Unreachable { .. }), "{error:?}");
    assert_eq!(error.effect(), OperationEffect::NotApplied);
}

#[tokio::test]
async fn a_name_that_will_not_resolve_reports_the_write_as_not_applied() {
    // DNS failure precedes delivery just as a refused connect does, and reqwest
    // reports both through `is_connect()`. `.invalid` is reserved by RFC 2606
    // and never resolves.
    let error = post_mutation("http://connector.invalid").await;

    assert!(matches!(error, ConnectorError::Unreachable { .. }), "{error:?}");
    assert_eq!(error.effect(), OperationEffect::NotApplied);
}

#[tokio::test]
async fn the_four_transport_failures_do_not_share_one_variant() {
    // The defect stated as a test: a connect refusal that certainly did not
    // apply used to be indistinguishable from two failures that certainly or
    // possibly did.
    let truncated = FakeConnector::start(Misbehaviour::TruncateBodyAfterOk).await;
    let silent = FakeConnector::start(Misbehaviour::NeverAnswer).await;

    let effects = [
        post_mutation(&dead_endpoint().await).await.effect(),
        post_mutation(silent.endpoint()).await.effect(),
        post_mutation(truncated.endpoint()).await.effect(),
    ];

    assert_eq!(
        effects,
        [
            OperationEffect::NotApplied,
            OperationEffect::Unknown,
            OperationEffect::Applied
        ]
    );
}
