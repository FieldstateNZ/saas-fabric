//! Layered authorization end to end (ADR 0027; D01-10, D01-12).
//!
//! The real router, a recording connector, and a scripted Check standing in
//! for the authorization front. No network: the Check is in process, and the
//! HTTP adapter's own wire behaviour is tested against a fake front in
//! `fabric-fga-check`.
//!
//! The fixture policies:
//!
//! | tenant | posture | declares relations on |
//! |---|---|---|
//! | `globex` | synthetic trial | `customers` |
//! | `acme` | relation-checked | `customers` |
//!
//! `readOnlyReport` declares no relations for either.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use fabric_core::{LogicalResourceName, TenantId};
use fabric_data_api::{
    AuthorizationCheck, CallerCredential, CheckDecision, CheckFailure, CheckQuestion, DataAuthorization,
    NoCheckConfigured, OperationKind, TenantAuthorization, TenantPolicies, TenantPosture,
};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::{body_json, request, RecordingConnector};
use tower::ServiceExt as _;

/// A Check that answers one scripted way and records each question asked.
struct ScriptedCheck {
    answer: Result<CheckDecision, CheckFailure>,
    asked: Mutex<Vec<(String, CheckQuestion)>>,
}

impl ScriptedCheck {
    fn answering(answer: Result<CheckDecision, CheckFailure>) -> Arc<Self> {
        Arc::new(Self {
            answer,
            asked: Mutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Vec<(String, CheckQuestion)> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl AuthorizationCheck for ScriptedCheck {
    async fn check(
        &self,
        credential: &CallerCredential,
        question: &CheckQuestion,
    ) -> Result<CheckDecision, CheckFailure> {
        self.asked
            .lock()
            .unwrap()
            .push((credential.bearer().to_owned(), question.clone()));
        self.answer
    }
}

fn customers() -> LogicalResourceName {
    LogicalResourceName::try_new("customers").unwrap()
}

fn policies() -> TenantPolicies {
    TenantPolicies::default()
        .with(
            TenantId::try_new("globex").unwrap(),
            TenantAuthorization::new(TenantPosture::SyntheticTrial, [customers()]),
        )
        .with(
            TenantId::try_new("acme").unwrap(),
            TenantAuthorization::new(TenantPosture::RelationChecked, [customers()]),
        )
}

/// The router over the fixture policies and the given Check.
fn app(check: Arc<dyn AuthorizationCheck>) -> (Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![support::row(1, "Alice")]);
    let runtime = support::resolver(support::tenants(), support::data_sources());
    let authorization = DataAuthorization::new(policies(), check);

    (
        support::app_with(runtime, Arc::clone(&connector), authorization),
        connector,
    )
}

/// Claims for `tenant` holding read and write on `customers` and `readOnlyReport`.
fn scoped(tenant: &str) -> Value {
    json!({
        "tenant_id": tenant,
        "scope": "data:customers:read data:customers:write data:readOnlyReport:read"
    })
}

/// Sends a request, returning the bearer it carried, the status and the body.
async fn send(app: Router, request: Request<axum::body::Body>) -> (String, StatusCode, Value) {
    let bearer = request.headers()["authorization"]
        .to_str()
        .unwrap()
        .trim_start_matches("Bearer ")
        .to_owned();
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();

    (bearer, status, body_json(response).await)
}

#[tokio::test]
async fn a_trial_tenant_is_allowed_by_scope_alone() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Denied));

    for uri in ["/customers/1", "/customers"] {
        let (app, connector) = app(Arc::clone(&check) as Arc<dyn AuthorizationCheck>);

        let (_, status, _) = send(app, request("GET", uri, scoped("globex"))).await;

        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(connector.query_count(), 1, "{uri}");
    }
    assert!(
        check.asked().is_empty(),
        "the trial never asks, even where relations are declared"
    );
}

#[tokio::test]
async fn a_tenant_beyond_the_trial_needs_a_check_allow_about_the_record() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));
    let (app, connector) = app(Arc::clone(&check) as Arc<dyn AuthorizationCheck>);

    let (bearer, status, _) = send(app, request("GET", "/customers/1", scoped("acme"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(connector.query_count(), 1);

    let asked = check.asked();
    let [(sent, question)] = asked.as_slice() else {
        panic!("exactly one Check, got {asked:?}");
    };
    assert_eq!(
        sent, &bearer,
        "the caller's own token, never a workload credential"
    );
    assert_eq!(question.operation, OperationKind::Read);
    assert_eq!(question.object.to_string(), "customers:1");
}

#[tokio::test]
async fn a_check_deny_is_refused_like_any_other_denial() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Denied));
    let (app, connector) = app(Arc::clone(&check) as Arc<dyn AuthorizationCheck>);

    let (_, status, body) = send(app, request("DELETE", "/customers/1", scoped("acme"))).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    assert_eq!(connector.mutation_count(), 0);
    assert_eq!(check.asked().len(), 1);
}

#[tokio::test]
async fn an_unreachable_check_is_refused() {
    let (app, connector) = app(ScriptedCheck::answering(Err(CheckFailure::Unavailable)));

    let (_, status, body) = send(app, request("GET", "/customers/1", scoped("acme"))).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "authorization_unavailable");
    assert_eq!(
        connector.query_count(),
        0,
        "fail closed: nothing reaches the connector"
    );
}

#[tokio::test]
async fn a_check_that_answers_nonsense_or_does_not_exist_is_refused() {
    for check in [
        ScriptedCheck::answering(Err(CheckFailure::Internal)) as Arc<dyn AuthorizationCheck>,
        Arc::new(NoCheckConfigured),
    ] {
        let (app, connector) = app(check);

        let (_, status, body) = send(app, request("GET", "/customers/1", scoped("acme"))).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"]["code"], "internal");
        assert_eq!(connector.query_count(), 0);
    }
}

#[tokio::test]
async fn a_resource_that_declares_no_relations_uses_scopes_only() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Denied));
    let (app, connector) = app(Arc::clone(&check) as Arc<dyn AuthorizationCheck>);

    let (_, status, _) = send(app, request("GET", "/readOnlyReport/1", scoped("acme"))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(connector.query_count(), 1);
    assert!(check.asked().is_empty());
}

#[tokio::test]
async fn a_missing_scope_is_refused_before_the_check_is_asked() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));
    let (app, _) = app(Arc::clone(&check) as Arc<dyn AuthorizationCheck>);

    let claims = json!({"tenant_id": "acme", "scope": "data:customers:read"});
    let (_, status, _) = send(app, request("DELETE", "/customers/1", claims)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        check.asked().is_empty(),
        "an allow from layer 2 cannot stand in for a scope"
    );
}

#[tokio::test]
async fn administrator_and_operator_roles_get_no_data_path_bypass() {
    let roles = json!(["platform-admin", "fabric-operator"]);

    // Without scopes: refused for the trial and beyond it alike.
    for tenant in ["globex", "acme"] {
        let (app, connector) = app(ScriptedCheck::answering(Ok(CheckDecision::Allowed)));
        let claims = json!({"tenant_id": tenant, "scope": "", "roles": roles});

        let (_, status, _) = send(app, request("DELETE", "/customers/1", claims)).await;

        assert_eq!(status, StatusCode::FORBIDDEN, "{tenant}");
        assert_eq!(connector.mutation_count(), 0, "{tenant}");
    }

    // With scopes, beyond the trial: the Check still decides.
    let (app, connector) = app(ScriptedCheck::answering(Ok(CheckDecision::Denied)));
    let mut claims = scoped("acme");
    claims["roles"] = roles;

    let (_, status, _) = send(app, request("DELETE", "/customers/1", claims)).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(connector.mutation_count(), 0);
}

#[tokio::test]
async fn a_collection_operation_on_a_checked_resource_is_refused_until_one_is_defined() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));

    for (method, uri) in [("GET", "/customers"), ("POST", "/customers")] {
        let (app, connector) = app(Arc::clone(&check) as Arc<dyn AuthorizationCheck>);
        let request = support::json_request(method, uri, scoped("acme"), &json!({"name": "Eve"}));

        let (_, status, _) = send(app, request).await;

        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(connector.query_count() + connector.mutation_count(), 0);
    }
    assert!(check.asked().is_empty());
}

#[tokio::test]
async fn a_check_deny_and_a_missing_scope_answer_identically() {
    let (denied_app, _) = app(ScriptedCheck::answering(Ok(CheckDecision::Denied)));
    let (unscoped_app, _) = app(ScriptedCheck::answering(Ok(CheckDecision::Allowed)));

    let (_, denied_status, mut denied) =
        send(denied_app, request("GET", "/customers/1", scoped("acme"))).await;
    let (_, unscoped_status, mut unscoped) = send(
        unscoped_app,
        request("GET", "/customers/1", json!({"tenant_id": "acme", "scope": ""})),
    )
    .await;

    // The request id differs per request by design; everything else must not.
    denied["error"]["request_id"] = Value::Null;
    unscoped["error"]["request_id"] = Value::Null;
    assert_eq!(denied_status, unscoped_status);
    assert_eq!(denied, unscoped);
}
