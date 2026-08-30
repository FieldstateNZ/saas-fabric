//! What the adapter actually sends, and how it reads what comes back.
//!
//! Two things are under test and they are different: that the trusted values
//! reach the wire **unchanged**, and that every way the service can fail is
//! mapped to something that is not a denial.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use fabric_fga_auth::{DecisionFailure, Decisions, OpenFgaDecisions};
use support::FakeOpenFga;

// Deliberately mixed case throughout. Every value here was lowercase in the
// first draft of these tests, which made them blind to a normalisation bug: a
// mutation adding `.to_lowercase()` to the subject passed every assertion.
// Identifiers this platform permits to carry case must be shown to survive it.
const STORE: &str = "01ACMESTORE";
const MODEL: &str = "01ACMEMODEL";
const USER: &str = "user:acme/CB606ddc-F148-4193-8875-a84EA6a85E6C";
const RELATION: &str = "billing_Admin";
const OBJECT: &str = "auditEvents:A-b_C.1";

fn adapter_over(fake: &FakeOpenFga) -> OpenFgaDecisions {
    OpenFgaDecisions::on_loopback(fake.port).expect("a client")
}

#[tokio::test]
async fn every_trusted_value_reaches_the_wire_unchanged() {
    let fake = FakeOpenFga::answering(200, r#"{"allowed":true}"#).await;
    let allowed = adapter_over(&fake)
        .check(STORE, MODEL, USER, RELATION, OBJECT)
        .await
        .expect("a decision");

    assert!(allowed);

    let sent = fake.only_request();

    // The store is a path segment, so a transformation here would be invisible
    // in the body.
    assert_eq!(sent.path, format!("/stores/{STORE}/check"));

    // Named explicitly rather than left to the service's default, which is its
    // most recent model.
    assert_eq!(sent.model().as_deref(), Some(MODEL));

    // The exact strings a SubjectId, a RelationName and an ObjectRef render
    // to. Any normalisation, escaping or case change between the operation and
    // the socket shows up here.
    assert_eq!(sent.tuple("user"), USER);
    assert_eq!(sent.tuple("relation"), RELATION);
    assert_eq!(sent.tuple("object"), OBJECT);
}

#[tokio::test]
async fn a_negative_decision_is_an_answer() {
    let fake = FakeOpenFga::answering(200, r#"{"allowed":false}"#).await;

    let allowed = adapter_over(&fake)
        .check(STORE, MODEL, USER, RELATION, OBJECT)
        .await
        .expect("not permitted is still a decision");

    assert!(!allowed);
}

#[tokio::test]
async fn a_failing_service_is_unavailable_and_never_a_denial() {
    for status in [500, 502, 503, 504] {
        let fake = FakeOpenFga::answering(status, r#"{"code":"internal_error"}"#).await;

        let failure = adapter_over(&fake)
            .check(STORE, MODEL, USER, RELATION, OBJECT)
            .await
            .expect_err("a 5xx is not a decision");

        assert_eq!(
            failure,
            DecisionFailure::Unavailable,
            "{status} must be unavailable"
        );
    }
}

#[tokio::test]
async fn a_refused_request_is_our_fault_and_never_a_denial() {
    // A 400 or a 404 means the store or model we named does not exist, or the
    // body we built is wrong. Everything in that request except the relation
    // and the object came from this platform, so the caller may well hold the
    // permission and nothing they do will help.
    for status in [400, 401, 403, 404, 422] {
        let fake = FakeOpenFga::answering(status, r#"{"code":"validation_error"}"#).await;

        let failure = adapter_over(&fake)
            .check(STORE, MODEL, USER, RELATION, OBJECT)
            .await
            .expect_err("a 4xx is not a decision");

        assert_eq!(failure, DecisionFailure::Internal, "{status} must be internal");
    }
}

#[tokio::test]
async fn an_answer_that_is_not_a_decision_is_internal() {
    for body in [r#"{"unexpected":true}"#, "not json at all", "", "{}"] {
        let fake = FakeOpenFga::answering(200, body).await;

        let failure = adapter_over(&fake)
            .check(STORE, MODEL, USER, RELATION, OBJECT)
            .await
            .expect_err("an unreadable answer is not a decision");

        assert_eq!(failure, DecisionFailure::Internal, "{body:?} must be internal");
    }
}

#[tokio::test]
async fn nothing_answering_at_all_is_unavailable() {
    // A port with no listener: connection refused, which is the ordinary shape
    // of the embedded service not being up yet.
    let free = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);

    let failure = OpenFgaDecisions::on_loopback(port)
        .expect("a client")
        .check(STORE, MODEL, USER, RELATION, OBJECT)
        .await
        .expect_err("nothing is listening");

    assert_eq!(failure, DecisionFailure::Unavailable);
}
