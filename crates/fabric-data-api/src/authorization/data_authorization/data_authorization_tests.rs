//! The order of the layers and the fail-closed rule, without HTTP.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_core::{LogicalResourceName, TenantId};
use serde_json::json;

use super::*;
use crate::authorization::caller::fixtures::caller_with;
use crate::{CallerCredential, CheckFailure, TenantAuthorization, TenantPosture};

/// A Check that answers one scripted way and records every question.
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

    fn questions(&self) -> Vec<(String, CheckQuestion)> {
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

/// `acme` is checked on `customers`; `trial` is the synthetic trial.
fn authorization(check: Arc<ScriptedCheck>) -> DataAuthorization {
    let policies = TenantPolicies::default()
        .with(
            TenantId::try_new("acme").unwrap(),
            TenantAuthorization::new(TenantPosture::RelationChecked, [customers()]),
        )
        .with(
            TenantId::try_new("trial").unwrap(),
            TenantAuthorization::new(TenantPosture::SyntheticTrial, [customers()]),
        );

    DataAuthorization::new(policies, check)
}

fn scoped(tenant: &str) -> Caller {
    caller_with(json!({"tenant_id": tenant, "scope": "data:customers:read data:customers:write"}))
}

#[tokio::test]
async fn the_check_is_asked_about_the_record_with_the_callers_own_token() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));

    let outcome = authorization(Arc::clone(&check))
        .decide(&scoped("acme"), OperationKind::Update, &customers(), Some("42"))
        .await;

    assert_eq!(outcome, Ok(()));
    let questions = check.questions();
    let [(token, question)] = questions.as_slice() else {
        panic!("exactly one question, got {questions:?}");
    };
    assert_eq!(token, "the-callers-own-token");
    assert_eq!(question.operation, OperationKind::Update);
    assert_eq!(question.object.to_string(), "customers:42");
}

#[tokio::test]
async fn a_missing_scope_refuses_before_the_check_is_asked() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));
    let caller = caller_with(json!({"tenant_id": "acme"}));

    let outcome = authorization(Arc::clone(&check))
        .decide(&caller, OperationKind::Read, &customers(), Some("42"))
        .await;

    assert_eq!(outcome, Err(Refusal::Denied(DenialReason::MissingScope)));
    assert!(check.questions().is_empty());
}

#[tokio::test]
async fn every_failure_to_decide_refuses() {
    for failure in [
        CheckFailure::Unavailable,
        CheckFailure::Internal,
        CheckFailure::NotConfigured,
    ] {
        let outcome = authorization(ScriptedCheck::answering(Err(failure)))
            .decide(&scoped("acme"), OperationKind::Read, &customers(), Some("42"))
            .await;

        assert_eq!(outcome, Err(Refusal::Undecided(failure)));
    }
}

#[tokio::test]
async fn an_operation_naming_no_record_is_refused_where_a_check_is_required() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));

    for operation in [OperationKind::List, OperationKind::Create] {
        let outcome = authorization(Arc::clone(&check))
            .decide(&scoped("acme"), operation, &customers(), None)
            .await;

        assert_eq!(
            outcome,
            Err(Refusal::Denied(DenialReason::NoObjectToCheck)),
            "{operation}"
        );
    }
    assert!(check.questions().is_empty());
}

#[tokio::test]
async fn a_key_that_cannot_name_an_object_is_refused_without_asking() {
    let check = ScriptedCheck::answering(Ok(CheckDecision::Allowed));

    let outcome = authorization(Arc::clone(&check))
        .decide(&scoped("acme"), OperationKind::Read, &customers(), Some("a#b"))
        .await;

    assert_eq!(outcome, Err(Refusal::Denied(DenialReason::UnnameableObject)));
    assert!(check.questions().is_empty());
}

#[tokio::test]
async fn the_default_holds_every_tenant_to_its_scopes_and_asks_nothing() {
    let outcome = DataAuthorization::default()
        .decide(&scoped("acme"), OperationKind::List, &customers(), None)
        .await;

    assert_eq!(outcome, Ok(()));
}

#[tokio::test]
async fn a_policy_without_a_configured_check_fails_closed() {
    let policies = TenantPolicies::default().with(
        TenantId::try_new("acme").unwrap(),
        TenantAuthorization::new(TenantPosture::RelationChecked, [customers()]),
    );
    let authorization = DataAuthorization::new(policies, Arc::new(NoCheckConfigured));

    let outcome = authorization
        .decide(&scoped("acme"), OperationKind::Read, &customers(), Some("42"))
        .await;

    assert_eq!(outcome, Err(Refusal::Undecided(CheckFailure::NotConfigured)));
}
