//! The written model against a real, pinned OpenFGA (ADR 0027 §2a).
//!
//! # Why this exists beside the unit tests
//!
//! The unit tests prove the JSON this crate writes. Only the service can prove
//! that it accepts that JSON and decides the way §2a needs:
//!
//! - zero, one and several contributing relations each resolve;
//! - "nobody" (first minus first) denies even to a holder of the first
//!   relation, on the record and on the collection;
//! - a record grant never answers a collection question, and the reverse;
//! - two resources with the same relation names never share a permission;
//! - a computed permission cannot be granted by a tuple.
//!
//! # How it runs
//!
//! CI's `authorization-model` job starts [`OPENFGA_IMAGE`] in a throwaway
//! container on the runner and sets `FABRIC_REQUIRE_OPENFGA=1`, which turns a
//! missing `FABRIC_TEST_OPENFGA_PORT` into a failure. Anywhere else the tests
//! skip, loudly. Locally:
//!
//! ```text
//! docker run -d -p 18200:8080 openfga/openfga:v1.19.0@sha256:78d1fa601d42340ecb131305d80d3767d0f254f9b1bc3646f9a557e11b24c63a run
//! FABRIC_REQUIRE_OPENFGA=1 FABRIC_TEST_OPENFGA_PORT=18200 cargo test -p fabric-fga-model --test model_against_openfga
//! ```
//!
//! Synthetic subjects only, and nothing addresses anything but `127.0.0.1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fabric_client_model::{AuthorizationConfiguration, Relation, ResourceAuthorization};
use fabric_core::{CollectionObject, LogicalResourceName, OperationKind, RelationName};
use fabric_fga_model::authorization_model;
use serde_json::{json, Value};

/// The server this proof runs against: the version the authorization front
/// image ships (`Dockerfile`, `authorization-source`), pinned by digest.
const OPENFGA_IMAGE: &str =
    "openfga/openfga:v1.19.0@sha256:78d1fa601d42340ecb131305d80d3767d0f254f9b1bc3646f9a557e11b24c63a";

/// Set by the CI job that owns this proof.
const REQUIRE_ENV: &str = "FABRIC_REQUIRE_OPENFGA";

/// The port a real OpenFGA listens on, or `None` to skip. Panics in required
/// mode, so the job cannot go green without a service answering.
fn configured_port() -> Option<u16> {
    let port = std::env::var("FABRIC_TEST_OPENFGA_PORT")
        .ok()
        .and_then(|value| value.parse().ok());

    if port.is_none() {
        assert!(
            std::env::var(REQUIRE_ENV).as_deref() != Ok("1"),
            "{REQUIRE_ENV}=1 but FABRIC_TEST_OPENFGA_PORT does not name a port: \
             the OpenFGA proof cannot run"
        );
        eprintln!("SKIPPED: set FABRIC_TEST_OPENFGA_PORT to run this against a real OpenFGA");
    }

    port
}

fn relation(name: &str, permits: &[OperationKind]) -> Relation {
    Relation {
        name: RelationName::try_new(name).unwrap(),
        permits: permits.to_vec(),
    }
}

fn resource(name: &str, relations: Vec<Relation>) -> ResourceAuthorization {
    ResourceAuthorization {
        resource: LogicalResourceName::try_new(name).unwrap(),
        relations,
    }
}

/// Three resources:
///
/// - `customers`, ADR 0013's example: several contributors (`can_read`,
///   `can_list`, `can_create`, `can_update`) and one (`can_delete`);
/// - `orders`, whose `viewer` permits only `read`, so the same relation name
///   means something different from `customers`' `viewer`;
/// - `auditEvents`, where nothing permits anything but `read`, declared
///   `zeta` before `alpha` so "first" is by name, not position.
fn configuration() -> AuthorizationConfiguration {
    use OperationKind::{Create, Delete, List, Read, Update};

    AuthorizationConfiguration {
        resources: vec![
            resource(
                "customers",
                vec![
                    relation("viewer", &[Read, List]),
                    relation("editor", &[Read, List, Create, Update]),
                    relation("owner", &[Read, List, Create, Update, Delete]),
                ],
            ),
            resource("orders", vec![relation("viewer", &[Read])]),
            resource(
                "auditEvents",
                vec![relation("zeta", &[Read]), relation("alpha", &[Read])],
            ),
        ],
    }
}

fn collection(resource: &str) -> String {
    CollectionObject::try_new(LogicalResourceName::try_new(resource).unwrap())
        .unwrap()
        .to_string()
}

/// A synthetic subject, spelled the way the front binds one.
fn subject(name: &str) -> String {
    format!("user:synthetic/{name}")
}

struct Store {
    http: reqwest::Client,
    base: String,
    model: String,
}

impl Store {
    /// A fresh store holding the model this crate writes.
    async fn new(port: u16) -> Self {
        let http = reqwest::Client::new();
        let root = format!("http://127.0.0.1:{port}");
        let store: Value = http
            .post(format!("{root}/stores"))
            .json(&json!({"name": "fabric-model-proof"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let base = format!("{root}/stores/{}", store["id"].as_str().unwrap());

        let body = authorization_model(&configuration())
            .unwrap()
            .expect("a managed model");
        let written = http
            .post(format!("{base}/authorization-models"))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = written.status();
        let answer: Value = written.json().await.unwrap();
        assert!(status.is_success(), "the service must accept the model: {answer}");
        let model = answer["authorization_model_id"].as_str().unwrap().to_owned();

        Self { http, base, model }
    }

    /// Writes one tuple, returning the status the service answered.
    async fn write(&self, user: &str, relation: &str, object: &str) -> reqwest::StatusCode {
        self.http
            .post(format!("{}/write", self.base))
            .json(&json!({
                "authorization_model_id": self.model,
                "writes": {"tuple_keys": [{"user": user, "relation": relation, "object": object}]}
            }))
            .send()
            .await
            .unwrap()
            .status()
    }

    /// Grants, failing the test if the service refuses.
    async fn grant(&self, user: &str, relation: &str, object: &str) {
        let status = self.write(user, relation, object).await;
        assert!(status.is_success(), "granting {relation} on {object}: {status}");
    }

    /// The decision, or the status the service refused the question with.
    async fn check(&self, user: &str, relation: &str, object: &str) -> Result<bool, u16> {
        let response = self
            .http
            .post(format!("{}/check", self.base))
            .json(&json!({
                "authorization_model_id": self.model,
                "tuple_key": {"user": user, "relation": relation, "object": object}
            }))
            .send()
            .await
            .unwrap();

        if !response.status().is_success() {
            return Err(response.status().as_u16());
        }

        Ok(response.json::<Value>().await.unwrap()["allowed"] == true)
    }
}

#[tokio::test]
async fn one_and_several_contributing_relations_resolve() {
    let Some(port) = configured_port() else { return };
    let store = Store::new(port).await;
    let customers = collection("customers");
    let (viewer, editor, owner) = (subject("viewer"), subject("editor"), subject("owner"));

    for (user, relation) in [(&viewer, "viewer"), (&editor, "editor"), (&owner, "owner")] {
        store.grant(user, relation, "customers:1").await;
        store.grant(user, relation, &customers).await;
    }

    // Several contributors: a union.
    for user in [&viewer, &editor, &owner] {
        assert_eq!(
            store.check(user, "can_read", "customers:1").await,
            Ok(true),
            "{user}"
        );
        assert_eq!(
            store.check(user, "can_list", &customers).await,
            Ok(true),
            "{user}"
        );
    }
    assert_eq!(store.check(&viewer, "can_update", "customers:1").await, Ok(false));
    assert_eq!(store.check(&editor, "can_update", "customers:1").await, Ok(true));
    assert_eq!(store.check(&viewer, "can_create", &customers).await, Ok(false));
    assert_eq!(store.check(&editor, "can_create", &customers).await, Ok(true));

    // One contributor: the relation itself.
    assert_eq!(store.check(&owner, "can_delete", "customers:1").await, Ok(true));
    assert_eq!(store.check(&editor, "can_delete", "customers:1").await, Ok(false));
}

#[tokio::test]
async fn nobody_denies_even_a_holder_of_the_first_relation_on_record_and_collection() {
    let Some(port) = configured_port() else { return };
    let store = Store::new(port).await;
    let audit = collection("auditEvents");
    let holder = subject("holder");

    for relation in ["alpha", "zeta"] {
        store.grant(&holder, relation, "auditEvents:1").await;
        store.grant(&holder, relation, &audit).await;
    }

    assert_eq!(store.check(&holder, "can_read", "auditEvents:1").await, Ok(true));
    for relation in ["can_update", "can_delete"] {
        assert_eq!(
            store.check(&holder, relation, "auditEvents:1").await,
            Ok(false),
            "{relation}"
        );
    }
    for relation in ["can_list", "can_create"] {
        assert_eq!(
            store.check(&holder, relation, &audit).await,
            Ok(false),
            "{relation}"
        );
    }
}

#[tokio::test]
async fn record_and_collection_grants_never_answer_for_each_other() {
    let Some(port) = configured_port() else { return };
    let store = Store::new(port).await;
    let customers = collection("customers");
    let (record_owner, collection_owner) = (subject("record-owner"), subject("collection-owner"));

    store.grant(&record_owner, "owner", "customers:42").await;
    store.grant(&collection_owner, "owner", &customers).await;

    assert_eq!(
        store.check(&record_owner, "can_delete", "customers:42").await,
        Ok(true)
    );
    assert_eq!(
        store.check(&record_owner, "can_list", &customers).await,
        Ok(false)
    );
    assert_eq!(
        store.check(&record_owner, "can_create", &customers).await,
        Ok(false)
    );

    assert_eq!(
        store.check(&collection_owner, "can_list", &customers).await,
        Ok(true)
    );
    for relation in ["can_read", "can_update", "can_delete"] {
        assert_eq!(
            store.check(&collection_owner, relation, "customers:42").await,
            Ok(false),
            "{relation}"
        );
    }

    // Neither type defines the other's operations: the question is refused.
    assert!(store
        .check(&record_owner, "can_list", "customers:42")
        .await
        .is_err());
    assert!(store
        .check(&collection_owner, "can_read", &customers)
        .await
        .is_err());
}

#[tokio::test]
async fn resources_with_the_same_relation_names_share_no_permission() {
    let Some(port) = configured_port() else { return };
    let store = Store::new(port).await;
    let (customers, orders) = (collection("customers"), collection("orders"));
    let viewer = subject("viewer");

    store.grant(&viewer, "viewer", &customers).await;
    store.grant(&viewer, "viewer", "customers:1").await;
    store.grant(&viewer, "viewer", &orders).await;

    assert_eq!(store.check(&viewer, "can_list", &customers).await, Ok(true));
    // `orders`' viewer permits only `read`, whatever `customers`' permits.
    assert_eq!(store.check(&viewer, "can_list", &orders).await, Ok(false));
    // A `customers` grant says nothing about any `orders` record.
    assert_eq!(store.check(&viewer, "can_read", "orders:1").await, Ok(false));
}

#[tokio::test]
async fn a_computed_permission_cannot_be_granted_by_a_tuple() {
    let Some(port) = configured_port() else { return };
    let store = Store::new(port).await;
    let user = subject("forger");
    let customers = collection("customers");

    for (relation, object) in [
        ("can_read", "customers:1"),
        ("can_update", "customers:1"),
        ("can_delete", "customers:1"),
        ("can_list", customers.as_str()),
        ("can_create", customers.as_str()),
        ("can_list", "auditEvents_collection:auditEvents"),
    ] {
        let status = store.write(&user, relation, object).await;

        assert!(status.is_client_error(), "{relation} on {object}: {status}");
    }
}

#[test]
fn ci_runs_the_pinned_server_in_required_mode() {
    let ci = include_str!("../../../.github/workflows/ci.yml");

    assert!(ci.contains(OPENFGA_IMAGE), "ci.yml must start {OPENFGA_IMAGE}");
    assert!(
        ci.contains("FABRIC_REQUIRE_OPENFGA: \"1\""),
        "ci.yml must set {REQUIRE_ENV}"
    );
}
