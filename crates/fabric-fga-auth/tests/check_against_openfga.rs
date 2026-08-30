//! The adapter against a real OpenFGA.
//!
//! # Why this exists beside the socket tests
//!
//! `check_over_the_wire` proves what goes out: the store in the path, the
//! model in the body, the exact rendered subject, relation and object. It
//! cannot prove that a real service *agrees* with any of it — that the field
//! names are the ones OpenFGA reads, that a model built from Fabric's
//! vocabulary resolves, or that a tuple written for
//! `user:<realm>/<subject>` is the one a check finds.
//!
//! # How to run it
//!
//! Skipped unless `FABRIC_TEST_OPENFGA_PORT` names a running OpenFGA:
//!
//! ```text
//! docker run -d -p 18200:8080 openfga/openfga:latest run
//! FABRIC_TEST_OPENFGA_PORT=18200 cargo test -p fabric-fga-auth --test check_against_openfga
//! ```
//!
//! Skipping rather than failing when it is absent, because an unconfigured
//! developer machine is not a broken build — but a skip that nobody notices is
//! a test that silently stopped running, so it says so loudly.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fabric_fga_auth::{DecisionFailure, Decisions, OpenFgaDecisions};

/// The port a real OpenFGA is listening on, if a test run provided one.
fn configured_port() -> Option<u16> {
    std::env::var("FABRIC_TEST_OPENFGA_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
}

/// Creates a store and a model, and grants `viewer` to one subject.
///
/// Returns the store and model ids the adapter should be given.
async fn prepared(port: u16, subject: &str) -> (String, String) {
    let http = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");

    let store: serde_json::Value = http
        .post(format!("{base}/stores"))
        .json(&serde_json::json!({ "name": "fabric-adapter-test" }))
        .send()
        .await
        .expect("create store")
        .json()
        .await
        .expect("store json");
    let store_id = store["id"].as_str().expect("a store id").to_owned();

    // The shape ADR 0013's vocabulary produces: a relation somebody holds, and
    // a computed relation naming what it permits.
    let model: serde_json::Value = http
        .post(format!("{base}/stores/{store_id}/authorization-models"))
        .json(&serde_json::json!({
            "schema_version": "1.1",
            "type_definitions": [
                { "type": "user" },
                {
                    "type": "auditEvents",
                    "relations": {
                        "viewer": { "this": {} },
                        "can_read": { "computedUserset": { "relation": "viewer" } }
                    },
                    "metadata": { "relations": {
                        "viewer": { "directly_related_user_types": [{ "type": "user" }] },
                        "can_read": { "directly_related_user_types": [] }
                    }}
                }
            ]
        }))
        .send()
        .await
        .expect("create model")
        .json()
        .await
        .expect("model json");
    let model_id = model["authorization_model_id"]
        .as_str()
        .expect("a model id")
        .to_owned();

    http.post(format!("{base}/stores/{store_id}/write"))
        .json(&serde_json::json!({
            "authorization_model_id": model_id,
            "writes": { "tuple_keys": [
                { "user": subject, "relation": "viewer", "object": "auditEvents:a-b_c.1" }
            ]}
        }))
        .send()
        .await
        .expect("write tuple");

    (store_id, model_id)
}

#[tokio::test]
async fn a_real_openfga_agrees_with_what_the_adapter_sends() {
    let Some(port) = configured_port() else {
        eprintln!(
            "SKIPPED: set FABRIC_TEST_OPENFGA_PORT to run this against a real OpenFGA \
             (see this file's documentation)"
        );
        return;
    };

    // Realm-qualified, exactly as `SubjectId` renders — and deliberately
    // mixed case, because a real engine treats identifiers as opaque and any
    // normalisation on our side would silently stop the tuple matching.
    let granted = "user:acme/CB606ddc-F148-4193-8875-a84EA6a85E6C";
    let (store, model) = prepared(port, granted).await;
    let adapter = OpenFgaDecisions::on_loopback(port).expect("a client");

    // The relation the tuple granted.
    assert!(
        adapter
            .check(&store, &model, granted, "viewer", "auditEvents:a-b_c.1")
            .await
            .expect("a decision"),
        "the subject the tuple names must be permitted"
    );

    // The computed relation, which is how ADR 0013's `permits` list will be
    // modelled — proving the adapter is not limited to direct relations.
    assert!(
        adapter
            .check(&store, &model, granted, "can_read", "auditEvents:a-b_c.1")
            .await
            .expect("a decision"),
        "a computed relation must resolve"
    );

    // The same subject in a different realm is a different subject. This is
    // the qualification from ADR 0015 doing its job against a real engine.
    assert!(
        !adapter
            .check(
                &store,
                &model,
                "user:other/CB606ddc-F148-4193-8875-a84EA6a85E6C",
                "viewer",
                "auditEvents:a-b_c.1"
            )
            .await
            .expect("a decision"),
        "a realm-qualified subject must not match another realm's"
    );

    // A different object nobody was granted.
    assert!(
        !adapter
            .check(&store, &model, granted, "viewer", "auditEvents:someone-elses")
            .await
            .expect("a decision"),
        "an ungranted object must not be permitted"
    );
}

#[tokio::test]
async fn a_store_that_does_not_exist_is_internal_and_never_a_denial() {
    let Some(port) = configured_port() else {
        eprintln!("SKIPPED: set FABRIC_TEST_OPENFGA_PORT to run this against a real OpenFGA");
        return;
    };

    let adapter = OpenFgaDecisions::on_loopback(port).expect("a client");

    // A real OpenFGA answering a real 4xx, which is the case the socket tests
    // can only simulate. Naming a store we do not have is a fault in this
    // platform's trusted state, not a caller lacking permission.
    let failure = adapter
        .check(
            "01JUNKSTOREIDTHATCANNOTEXIST",
            "01JUNKMODELID",
            "user:acme/someone",
            "viewer",
            "auditEvents:1",
        )
        .await
        .expect_err("an unknown store is not a decision");

    assert_eq!(failure, DecisionFailure::Internal);
}
