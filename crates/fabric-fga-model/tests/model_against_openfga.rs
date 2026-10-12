//! The written model against a real OpenFGA.
//!
//! # Why this exists beside the unit tests
//!
//! The unit tests prove the JSON this crate writes. They cannot prove that the
//! authorization service accepts it, or that it decides the way ADR 0027 §2a
//! needs: a collection grant answers `list` and `create`, a record grant never
//! does, and an operation no relation permits is a denial rather than an
//! error.
//!
//! # How to run it
//!
//! Skipped unless `FABRIC_TEST_OPENFGA_PORT` names a running OpenFGA, exactly
//! as `fabric-fga-auth`'s `check_against_openfga` is:
//!
//! ```text
//! docker run -d -p 18200:8080 openfga/openfga:latest run
//! FABRIC_TEST_OPENFGA_PORT=18200 cargo test -p fabric-fga-model --test model_against_openfga
//! ```
//!
//! A throwaway local container, never a deployment: nothing here addresses
//! anything but `127.0.0.1`.

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

fn configured_port() -> Option<u16> {
    std::env::var("FABRIC_TEST_OPENFGA_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
}

fn relation(name: &str, permits: &[OperationKind]) -> Relation {
    Relation {
        name: RelationName::try_new(name).unwrap(),
        permits: permits.to_vec(),
    }
}

/// `customers` as ADR 0013 declares it, and `auditEvents`, which nothing may
/// list or create.
fn configuration() -> AuthorizationConfiguration {
    use OperationKind::{Create, Delete, List, Read, Update};

    AuthorizationConfiguration {
        resources: vec![
            ResourceAuthorization {
                resource: LogicalResourceName::try_new("customers").unwrap(),
                relations: vec![
                    relation("viewer", &[Read, List]),
                    relation("editor", &[Read, List, Create, Update]),
                    relation("owner", &[Read, List, Create, Update, Delete]),
                ],
            },
            ResourceAuthorization {
                resource: LogicalResourceName::try_new("auditEvents").unwrap(),
                relations: vec![relation("viewer", &[Read])],
            },
        ],
    }
}

fn collection(resource: &str) -> String {
    CollectionObject::try_new(LogicalResourceName::try_new(resource).unwrap())
        .unwrap()
        .to_string()
}

struct Store {
    http: reqwest::Client,
    base: String,
    model: String,
}

impl Store {
    /// A fresh store holding the model this crate writes, and `tuples`.
    async fn with(port: u16, tuples: &[(&str, &str, &str)]) -> Self {
        let http = reqwest::Client::new();
        let root = format!("http://127.0.0.1:{port}");
        let store: Value = http
            .post(format!("{root}/stores"))
            .json(&json!({"name": "fabric-model-test"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let base = format!("{root}/stores/{}", store["id"].as_str().unwrap());

        let written = http
            .post(format!("{base}/authorization-models"))
            .json(&authorization_model(&configuration()).unwrap())
            .send()
            .await
            .unwrap();
        assert!(
            written.status().is_success(),
            "the service must accept the model: {}",
            written.text().await.unwrap()
        );
        let model = written.json::<Value>().await.unwrap()["authorization_model_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let keys: Vec<Value> = tuples
            .iter()
            .map(|(user, relation, object)| json!({"user": user, "relation": relation, "object": object}))
            .collect();
        let wrote = http
            .post(format!("{base}/write"))
            .json(&json!({"authorization_model_id": model, "writes": {"tuple_keys": keys}}))
            .send()
            .await
            .unwrap();
        assert!(wrote.status().is_success(), "{}", wrote.text().await.unwrap());

        Self { http, base, model }
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

const COLLECTION_VIEWER: &str = "user:acme/alice";
const RECORD_OWNER: &str = "user:acme/bob";

#[tokio::test]
async fn a_real_openfga_decides_collections_and_records_apart() {
    let Some(port) = configured_port() else {
        eprintln!(
            "SKIPPED: set FABRIC_TEST_OPENFGA_PORT to run this against a real OpenFGA \
             (see this file's documentation)"
        );
        return;
    };
    let customers = collection("customers");
    let store = Store::with(
        port,
        &[
            (COLLECTION_VIEWER, "viewer", &customers),
            (RECORD_OWNER, "owner", "customers:42"),
            (RECORD_OWNER, "viewer", "auditEvents:1"),
        ],
    )
    .await;

    // A collection grant answers the collection operations it permits.
    assert_eq!(
        store.check(COLLECTION_VIEWER, "can_list", &customers).await,
        Ok(true)
    );
    assert_eq!(
        store.check(COLLECTION_VIEWER, "can_create", &customers).await,
        Ok(false)
    );

    // A record grant, even `owner`, is never borrowed for the collection.
    assert_eq!(store.check(RECORD_OWNER, "can_list", &customers).await, Ok(false));
    assert_eq!(
        store.check(RECORD_OWNER, "can_create", &customers).await,
        Ok(false)
    );
    assert_eq!(
        store.check(RECORD_OWNER, "can_delete", "customers:42").await,
        Ok(true)
    );

    // A collection grant implies nothing about any record.
    assert_eq!(
        store.check(COLLECTION_VIEWER, "can_read", "customers:42").await,
        Ok(false)
    );

    // Neither type defines the other's operations, so the question is refused
    // rather than answered.
    assert!(store
        .check(RECORD_OWNER, "can_list", "customers:42")
        .await
        .is_err());
    assert!(store
        .check(COLLECTION_VIEWER, "can_read", &customers)
        .await
        .is_err());

    // An operation no relation permits is a denial, not a model error.
    let audit = collection("auditEvents");
    assert_eq!(
        store.check(RECORD_OWNER, "can_read", "auditEvents:1").await,
        Ok(true)
    );
    assert_eq!(store.check(RECORD_OWNER, "can_list", &audit).await, Ok(false));
    assert_eq!(
        store.check(RECORD_OWNER, "can_delete", "auditEvents:1").await,
        Ok(false)
    );
}

#[tokio::test]
async fn a_computed_relation_cannot_be_granted_directly() {
    let Some(port) = configured_port() else {
        eprintln!("SKIPPED: set FABRIC_TEST_OPENFGA_PORT (see this file's documentation)");
        return;
    };
    let store = Store::with(port, &[]).await;

    let wrote = store
        .http
        .post(format!("{}/write", store.base))
        .json(&json!({
            "authorization_model_id": store.model,
            "writes": {"tuple_keys": [
                {"user": COLLECTION_VIEWER, "relation": "can_list", "object": collection("customers")}
            ]}
        }))
        .send()
        .await
        .unwrap();

    assert!(wrote.status().is_client_error(), "{}", wrote.status());
}
