//! What `Check` sends, and what a caller cannot make it send.
//!
//! The interesting assertions are about the *request that goes out*, not the
//! answer that comes back: the whole point of the operation is that the
//! subject and the store are not the caller's to choose.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_core::{RelationName, SubjectId};

use crate::{Check, CheckRequest, DecisionError, DecisionFailure, Decisions, ObjectRef, VerifiedIdentity};

/// What the operation asked the authorization service.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Asked {
    store: String,
    model: String,
    user: String,
    relation: String,
    object: String,
}

/// Records the question and answers however the test says.
struct Recording {
    answer: Result<bool, DecisionFailure>,
    asked: Mutex<Option<Asked>>,
}

impl Recording {
    fn answering(answer: Result<bool, DecisionFailure>) -> Arc<Self> {
        Arc::new(Self {
            answer,
            asked: Mutex::new(None),
        })
    }

    fn asked(&self) -> Asked {
        self.asked
            .lock()
            .expect("not poisoned")
            .clone()
            .expect("the operation should have asked")
    }
}

#[async_trait]
impl Decisions for Recording {
    async fn check(
        &self,
        store: &str,
        model: &str,
        user: &str,
        relation: &str,
        object: &str,
    ) -> Result<bool, DecisionFailure> {
        *self.asked.lock().expect("not poisoned") = Some(Asked {
            store: store.to_owned(),
            model: model.to_owned(),
            user: user.to_owned(),
            relation: relation.to_owned(),
            object: object.to_owned(),
        });

        self.answer
    }
}

/// An identity as the verifier would have produced it.
fn alice() -> VerifiedIdentity {
    let principal = SubjectId::from_verified("acme", "alice-sub").expect("a valid principal");

    VerifiedIdentity::new(
        "acme".to_owned(),
        "alice-sub".to_owned(),
        principal,
        "01ACMESTORE".to_owned(),
        "01ACMEMODEL".to_owned(),
    )
}

fn request(relation: &str, object: &str) -> CheckRequest {
    CheckRequest {
        relation: RelationName::try_new(relation).expect("a valid relation"),
        object: ObjectRef::parse(object).expect("a valid object"),
    }
}

#[tokio::test]
async fn the_subject_asked_about_is_the_authenticated_caller() {
    let decisions = Recording::answering(Ok(true));
    let check = Check::new(Arc::clone(&decisions) as Arc<dyn Decisions>);

    let allowed = check
        .run(&alice(), &request("viewer", "document:123"))
        .await
        .expect("a decision");

    assert!(allowed);
    assert_eq!(
        decisions.asked(),
        Asked {
            // From the verified identity, which came from the registry.
            store: "01ACMESTORE".to_owned(),
            model: "01ACMEMODEL".to_owned(),
            user: "user:acme/alice-sub".to_owned(),
            // From the request, which is all the caller controls.
            relation: "viewer".to_owned(),
            object: "document:123".to_owned(),
        }
    );
}

#[test]
fn a_request_naming_a_user_is_refused_rather_than_ignored() {
    // The structural property. There is no `user` field to overwrite, so a
    // caller trying to name their own principal gets an error instead of a
    // decision that quietly ignored them.
    let refused = serde_json::from_str::<CheckRequest>(
        r#"{"relation":"viewer","object":"document:123","user":"user:acme/bob"}"#,
    );

    assert!(refused.is_err(), "a request naming a user must be refused");
}

#[test]
fn a_request_naming_a_store_a_tenant_or_a_principal_is_refused() {
    for field in ["store", "store_id", "tenant", "realm", "principal"] {
        let body = format!(r#"{{"relation":"viewer","object":"document:1","{field}":"anything"}}"#);

        assert!(
            serde_json::from_str::<CheckRequest>(&body).is_err(),
            "a request naming {field} must be refused"
        );
    }
}

#[test]
fn the_only_fields_are_relation_and_object() {
    let parsed: CheckRequest = serde_json::from_str(r#"{"relation":"editor","object":"customers:42"}"#)
        .expect("the Fabric shape parses");

    assert_eq!(parsed.relation.as_str(), "editor");
    assert_eq!(parsed.object.to_string(), "customers:42");
}

#[tokio::test]
async fn a_refusal_is_an_answer_and_an_outage_is_not() {
    let denied = Recording::answering(Ok(false));
    let allowed = Check::new(Arc::clone(&denied) as Arc<dyn Decisions>)
        .run(&alice(), &request("viewer", "document:123"))
        .await
        .expect("not permitted is still a decision");

    assert!(!allowed, "a denial is Ok(false), not an error");

    let broken = Recording::answering(Err(DecisionFailure::Unavailable));
    let error = Check::new(broken as Arc<dyn Decisions>)
        .run(&alice(), &request("viewer", "document:123"))
        .await
        .expect_err("an unreachable service is not a denial");

    // A caller told "denied" would go and ask an administrator for access they
    // already have. The distinction is the operator's only signal.
    assert_eq!(error, DecisionError::Unavailable);
}

#[test]
fn an_object_must_be_a_resource_and_an_id() {
    for bad in [
        "no-colon",
        ":no-type",
        "document:",
        "document:with space",
        "document:has#userset",
        "document:has/separator",
        "Not A Type:1",
    ] {
        assert!(
            ObjectRef::parse(bad).is_err(),
            "{bad:?} must not parse as an object"
        );
    }

    assert!(ObjectRef::parse("customers:123").is_ok());
    assert!(ObjectRef::parse("auditEvents:a-b_c.1").is_ok());
}

#[test]
fn an_object_names_the_same_resource_the_catalogue_does() {
    let object = ObjectRef::parse("customers:123").expect("valid");

    assert_eq!(object.resource().as_str(), "customers");
}
