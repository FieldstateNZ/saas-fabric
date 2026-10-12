//! The adapter against a fake authorization front on loopback.
//!
//! A real socket rather than a mocked client, for the reason
//! `fabric-fga-auth`'s own wire tests give: what is worth testing is what
//! actually goes out — the path, the bearer, the relation and the object —
//! and a mocked client would let a transformation creep in unseen. Nothing
//! leaves `127.0.0.1`, and no OpenFGA, certificate or credential is involved.
//!
//! These are unit tests on purpose. The fake speaks plain HTTP, and the only
//! way to point the adapter at plain HTTP is `FrontEndpoint::loopback_plaintext`,
//! which exists in this crate's test builds and nowhere else (ADR 0027 §2b).

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use fabric_core::LogicalResourceName;
use fabric_data_api::{
    AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckObject, CheckQuestion,
    CheckTarget, CollectionObject, OperationKind,
};
use serde_json::Value;
use tokio::net::TcpListener;

use crate::{FrontCheck, FrontEndpoint};

/// What the fake front saw: the `Authorization` header and the body.
type Seen = Arc<Mutex<Vec<(String, Value)>>>;

/// A fake front answering every `POST /v1/check` with `status` and `body`.
struct FakeFront {
    address: SocketAddr,
    seen: Seen,
}

impl FakeFront {
    async fn answering(status: StatusCode, body: &'static str) -> Self {
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/v1/check",
                post(
                    move |State(seen): State<Seen>, headers: HeaderMap, Json(request): Json<Value>| async move {
                        let authorization = headers
                            .get("authorization")
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .to_owned();
                        seen.lock().unwrap().push((authorization, request));
                        (status, [("content-type", "application/json")], body)
                    },
                ),
            )
            .with_state(Arc::clone(&seen));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        Self { address, seen }
    }

    /// The fake, through the test-only plaintext path.
    fn endpoint(&self) -> FrontEndpoint {
        FrontEndpoint::loopback_plaintext(self.address).unwrap()
    }

    fn seen(&self) -> Vec<(String, Value)> {
        self.seen.lock().unwrap().clone()
    }
}

fn collection(operation: OperationKind) -> CheckQuestion {
    CheckQuestion {
        operation,
        target: CheckTarget::Collection(
            CollectionObject::try_new(LogicalResourceName::try_new("customers").unwrap()).unwrap(),
        ),
    }
}

fn record(operation: OperationKind, id: &str) -> CheckQuestion {
    CheckQuestion {
        operation,
        target: CheckTarget::Record(
            CheckObject::try_new(LogicalResourceName::try_new("customers").unwrap(), id).unwrap(),
        ),
    }
}

/// Mixed case on purpose: an accidental `.to_lowercase()` anywhere on the way
/// out would change it.
fn credential() -> CallerCredential {
    CallerCredential::new("eyJ.Caller-Own.Token".to_owned())
}

async fn ask(endpoint: &FrontEndpoint, question: &CheckQuestion) -> Result<CheckDecision, CheckFailure> {
    FrontCheck::new(endpoint.clone())
        .unwrap()
        .check(&credential(), question)
        .await
}

#[tokio::test]
async fn a_200_allow_carries_the_callers_own_token_and_the_computed_relation_unchanged() {
    let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":true}"#).await;

    let decision = ask(&front.endpoint(), &record(OperationKind::Update, "Rec-42")).await;

    assert_eq!(decision, Ok(CheckDecision::Allowed));
    let seen = front.seen();
    let [(authorization, body)] = seen.as_slice() else {
        panic!("exactly one request, got {seen:?}");
    };
    assert_eq!(authorization, "Bearer eyJ.Caller-Own.Token");
    assert_eq!(
        body,
        &serde_json::json!({"relation": "can_update", "object": "customers:Rec-42"})
    );
}

#[tokio::test]
async fn a_200_false_answer_is_a_denial() {
    let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":false}"#).await;

    assert_eq!(
        ask(&front.endpoint(), &record(OperationKind::Read, "1")).await,
        Ok(CheckDecision::Denied)
    );
}

#[tokio::test]
async fn an_allow_body_under_any_success_status_but_200_is_refused() {
    // 201 and 202 are successes, but not the front's completed decision. The
    // allow body must not become access.
    for status in [StatusCode::CREATED, StatusCode::ACCEPTED] {
        let front = FakeFront::answering(status, r#"{"allowed":true}"#).await;

        assert_eq!(
            ask(&front.endpoint(), &record(OperationKind::Read, "1")).await,
            Err(CheckFailure::Internal),
            "{status}"
        );
        assert_eq!(
            front.seen().len(),
            1,
            "the request was sent; the answer was refused"
        );
    }
}

#[tokio::test]
async fn a_front_that_cannot_reach_its_authorization_service_is_unavailable() {
    let front = FakeFront::answering(StatusCode::SERVICE_UNAVAILABLE, r#"{"allowed":true}"#).await;

    assert_eq!(
        ask(&front.endpoint(), &record(OperationKind::Read, "1")).await,
        Err(CheckFailure::Unavailable)
    );
}

#[tokio::test]
async fn an_unreachable_front_is_unavailable_never_a_decision() {
    // Bind, learn the port, close: nothing is listening there any more.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let endpoint = FrontEndpoint::loopback_plaintext(address).unwrap();

    assert_eq!(
        ask(&endpoint, &record(OperationKind::Read, "1")).await,
        Err(CheckFailure::Unavailable)
    );
}

#[tokio::test]
async fn an_answer_that_is_not_a_decision_is_internal_never_a_decision() {
    for (status, body) in [
        // The front refused a token the runtime's ingress accepted.
        (StatusCode::UNAUTHORIZED, "{}"),
        (StatusCode::BAD_REQUEST, "{}"),
        // A 403 is not how the front denies, so it is not read as one.
        (StatusCode::FORBIDDEN, r#"{"allowed":true}"#),
        (StatusCode::TEMPORARY_REDIRECT, r#"{"allowed":true}"#),
        (StatusCode::OK, "not json"),
        (StatusCode::OK, r#"{"allowed":"yes"}"#),
    ] {
        let front = FakeFront::answering(status, body).await;

        assert_eq!(
            ask(&front.endpoint(), &record(OperationKind::Read, "1")).await,
            Err(CheckFailure::Internal),
            "{status} {body}"
        );
    }
}

#[tokio::test]
async fn each_operation_asks_its_own_computed_relation_about_its_own_kind_of_object() {
    let expected = [
        (record(OperationKind::Read, "42"), "can_read", "customers:42"),
        (record(OperationKind::Update, "42"), "can_update", "customers:42"),
        (record(OperationKind::Delete, "42"), "can_delete", "customers:42"),
        (
            collection(OperationKind::List),
            "can_list",
            "customers_collection:customers",
        ),
        (
            collection(OperationKind::Create),
            "can_create",
            "customers_collection:customers",
        ),
    ];

    for (question, relation, object) in expected {
        let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":true}"#).await;

        assert_eq!(
            ask(&front.endpoint(), &question).await,
            Ok(CheckDecision::Allowed)
        );
        let seen = front.seen();
        let [(authorization, body)] = seen.as_slice() else {
            panic!("exactly one request, got {seen:?}");
        };
        assert_eq!(authorization, "Bearer eyJ.Caller-Own.Token");
        assert_eq!(body, &serde_json::json!({"relation": relation, "object": object}));
    }
}

#[tokio::test]
async fn a_collection_answer_is_read_by_the_same_rules_as_a_record_answer() {
    for operation in [OperationKind::List, OperationKind::Create] {
        for (status, body, expected) in [
            (StatusCode::OK, r#"{"allowed":false}"#, Ok(CheckDecision::Denied)),
            (
                StatusCode::ACCEPTED,
                r#"{"allowed":true}"#,
                Err(CheckFailure::Internal),
            ),
            (StatusCode::BAD_REQUEST, "{}", Err(CheckFailure::Internal)),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                r#"{"allowed":true}"#,
                Err(CheckFailure::Unavailable),
            ),
        ] {
            let front = FakeFront::answering(status, body).await;

            assert_eq!(
                ask(&front.endpoint(), &collection(operation)).await,
                expected,
                "{operation} {status} {body}"
            );
        }
    }
}

#[tokio::test]
async fn an_operation_asked_about_the_wrong_kind_of_object_is_refused_without_sending_anything() {
    // A record allow must never answer `list` or `create`, and a collection
    // allow must never answer `read`, `update` or `delete`.
    let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":true}"#).await;
    let customers = || LogicalResourceName::try_new("customers").unwrap();

    for operation in OperationKind::ALL {
        let mismatched = if operation.is_collection_operation() {
            CheckTarget::Record(CheckObject::try_new(customers(), "42").unwrap())
        } else {
            CheckTarget::Collection(CollectionObject::try_new(customers()).unwrap())
        };
        let question = CheckQuestion {
            operation,
            target: mismatched,
        };

        assert_eq!(
            ask(&front.endpoint(), &question).await,
            Err(CheckFailure::Internal),
            "{operation}"
        );
    }
    assert!(front.seen().is_empty());
}

#[tokio::test]
async fn a_live_https_endpoint_never_falls_back_to_plaintext() {
    // The fake speaks plain HTTP. Addressed as https, the TLS handshake fails
    // and nothing is sent: the token does not travel in the clear.
    let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":true}"#).await;
    let endpoint = FrontEndpoint::parse(&format!("https://{}", front.address)).unwrap();

    assert_eq!(
        ask(&endpoint, &record(OperationKind::Read, "1")).await,
        Err(CheckFailure::Unavailable)
    );
    assert!(front.seen().is_empty());
}
