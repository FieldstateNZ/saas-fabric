//! The adapter against a fake authorization front on loopback.
//!
//! A real socket rather than a mocked client, for the reason
//! `fabric-fga-auth`'s own wire tests give: what is worth testing is what
//! actually goes out — the path, the bearer, the relation and the object —
//! and a mocked client would let a transformation creep in unseen. Nothing
//! leaves `127.0.0.1`, and no OpenFGA or credential is involved.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use fabric_core::LogicalResourceName;
use fabric_data_api::{
    AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckObject, CheckQuestion,
    OperationKind,
};
use fabric_fga_check::{FrontCheck, FrontEndpoint};
use serde_json::Value;
use tokio::net::TcpListener;

/// What the fake front saw: the `Authorization` header and the body.
type Seen = Arc<Mutex<Vec<(String, Value)>>>;

/// A fake front answering every `POST /v1/check` with `status` and `body`.
struct FakeFront {
    endpoint: FrontEndpoint,
    seen: Seen,
}

impl FakeFront {
    async fn answering(status: StatusCode, body: &'static str) -> Self {
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/v1/check",
                post(move |State(seen): State<Seen>, headers: HeaderMap, Json(request): Json<Value>| async move {
                    let authorization = headers["authorization"].to_str().unwrap().to_owned();
                    seen.lock().unwrap().push((authorization, request));
                    (status, [("content-type", "application/json")], body)
                }),
            )
            .with_state(Arc::clone(&seen));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        Self {
            endpoint: FrontEndpoint::parse(&format!("http://{address}")).unwrap(),
            seen,
        }
    }

    fn seen(&self) -> Vec<(String, Value)> {
        self.seen.lock().unwrap().clone()
    }
}

fn question(operation: OperationKind, id: &str) -> CheckQuestion {
    CheckQuestion {
        operation,
        object: CheckObject::try_new(LogicalResourceName::try_new("customers").unwrap(), id).unwrap(),
    }
}

/// Mixed case on purpose: an accidental `.to_lowercase()` anywhere on the way
/// out would change it.
fn credential() -> CallerCredential {
    CallerCredential::new("eyJ.Caller-Own.Token".to_owned())
}

async fn ask(endpoint: &FrontEndpoint, operation: OperationKind) -> Result<CheckDecision, CheckFailure> {
    FrontCheck::new(endpoint.clone())
        .unwrap()
        .check(&credential(), &question(operation, "Rec-42"))
        .await
}

#[tokio::test]
async fn the_callers_own_token_and_the_computed_relation_go_out_unchanged() {
    let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":true}"#).await;

    let decision = ask(&front.endpoint, OperationKind::Update).await;

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
async fn a_false_answer_is_a_denial() {
    let front = FakeFront::answering(StatusCode::OK, r#"{"allowed":false}"#).await;

    assert_eq!(
        ask(&front.endpoint, OperationKind::Read).await,
        Ok(CheckDecision::Denied)
    );
}

#[tokio::test]
async fn a_front_that_cannot_reach_its_authorization_service_is_unavailable() {
    let front = FakeFront::answering(StatusCode::SERVICE_UNAVAILABLE, "{}").await;

    assert_eq!(
        ask(&front.endpoint, OperationKind::Read).await,
        Err(CheckFailure::Unavailable)
    );
}

#[tokio::test]
async fn an_unreachable_front_is_unavailable_never_a_decision() {
    // Bind, learn the port, close: nothing is listening there any more.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let endpoint = FrontEndpoint::parse(&format!("http://{address}")).unwrap();

    assert_eq!(
        ask(&endpoint, OperationKind::Read).await,
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
        (StatusCode::TEMPORARY_REDIRECT, "{}"),
        (StatusCode::OK, "not json"),
        (StatusCode::OK, r#"{"allowed":"yes"}"#),
    ] {
        let front = FakeFront::answering(status, body).await;

        assert_eq!(
            ask(&front.endpoint, OperationKind::Read).await,
            Err(CheckFailure::Internal),
            "{status} {body}"
        );
    }
}
