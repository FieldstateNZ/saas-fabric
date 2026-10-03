//! Which repositories a managed component's images are read from, and which
//! registry reads them (ADR 0026 section 5): the platform panel's component
//! rows, and `GET /api/integrations/registries/reads`.
//!
//! Both are what desired state pins beside what operators registered —
//! nothing here claims a read succeeded.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use fabric_platform_management::{
    ArtifactSource, Channel, ComponentDesired, DesiredRevision, Registry, UpdatePolicy, Version,
};
use http::StatusCode;
use serde_json::{json, Value};
use support::described_registry::{described_at, DescribedRegistry, CONTROL_PLANE, RUNTIME};
use support::platform_fixture::platform_binding_over;
use support::selection::{registries_held, registries_holding, Held};
use support::{
    as_operator, control_plane_with_registries, control_plane_with_resolution, no_resolution, send,
    TestControlPlane,
};

/// The version the described component runs.
const DESIRED: &str = "0.3.0-preview.2";

/// An image component on a host nobody registered.
const TOOLS: &str = "quay.io/acme/tools";

/// A component pinned at `DESIRED`, published as `source`.
fn pinned(name: &str, source: ArtifactSource) -> (String, ComponentDesired) {
    (
        name.to_owned(),
        ComponentDesired {
            version: Version::parse(DESIRED).unwrap(),
            channel: Channel::Preview,
            policy: UpdatePolicy::Manual,
            hold: None,
            source,
            revision: DesiredRevision::new("desired-1"),
        },
    )
}

/// A plane managing a described component on `ghcr.io`, a chart, and an
/// image component on `quay.io`, with `ghcr.io` registered holding the
/// described component's primary repository and a credential.
async fn managed() -> (
    TestControlPlane,
    Arc<fabric_platform_management::PlatformDesiredState>,
) {
    managed_with(Held::Live).await
}

/// As [`managed`], with `ghcr.io`'s registry held as `held` says.
async fn managed_with(
    held: Held,
) -> (
    TestControlPlane,
    Arc<fabric_platform_management::PlatformDesiredState>,
) {
    let registry = DescribedRegistry::default();
    registry.publish(DESIRED, "5320432aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let components = vec![
        described_at(DESIRED),
        pinned(
            "keycloak",
            ArtifactSource::Helm {
                repository: "https://charts.example.com".to_owned(),
                chart: "keycloak".to_owned(),
            },
        ),
        pinned(
            "tools",
            ArtifactSource::Oci {
                repositories: BTreeMap::from([("cli".to_owned(), TOOLS.to_owned())]),
            },
        ),
    ];
    let (platform, _fake) = platform_binding_over(Arc::new(registry) as Arc<dyn Registry>, components).await;
    let desired_state = Arc::clone(&platform.repository);
    let plane = control_plane_with_resolution(
        registries_held(&[RUNTIME], true, held).await,
        no_resolution(),
        Some(platform),
    );
    (plane, desired_state)
}

/// What `path` answers.
async fn read(plane: &TestControlPlane, path: &str) -> Value {
    let response = send(
        &plane.router,
        as_operator("GET", path).body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    support::json(response).await
}

/// The row for `component`.
fn row<'a>(body: &'a Value, component: &str) -> &'a Value {
    body["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["component"] == component)
        .unwrap_or_else(|| panic!("no row for {component} in {body}"))
}

#[tokio::test]
async fn each_platform_component_row_names_its_image_repositories_by_role() {
    let (plane, _) = managed().await;

    let body = read(&plane, "/api/platform").await;

    assert_eq!(
        row(&body, "saas-fabric")["images"],
        json!({"controlPlane": CONTROL_PLANE, "runtime": RUNTIME})
    );
    assert_eq!(row(&body, "tools")["images"], json!({"cli": TOOLS}));
    assert_eq!(
        row(&body, "keycloak")["images"],
        Value::Null,
        "a chart has no images"
    );
    // What was there is unchanged.
    assert_eq!(row(&body, "saas-fabric")["artifact"], "oci");
    assert_eq!(row(&body, "keycloak")["artifact"], "helm");
}

/// What the reads route says of `ghcr.io`'s runtime image, and the host.
async fn ghcr_reads(held: Held) -> (Value, Value) {
    let (plane, _) = managed_with(held).await;
    let body = read(&plane, "/api/integrations/registries/reads").await;
    let host = body["hosts"][0].clone();
    assert_eq!(host["host"], "ghcr.io", "{body}");
    let runtime = host["images"][1].clone();
    assert_eq!(runtime["repository"], RUNTIME, "{body}");
    (host, runtime)
}

#[tokio::test]
async fn the_reads_route_says_which_registry_reads_each_image_and_how() {
    let (plane, _) = managed().await;

    let body = read(&plane, "/api/integrations/registries/reads").await;

    assert_eq!(
        body,
        json!({
            "state": "observed",
            "hosts": [
                {"host": "ghcr.io", "registered": true, "installed": true, "deployment": false, "images": [
                    {"component": "saas-fabric", "role": "controlPlane", "repository": CONTROL_PLANE,
                     "registered": false, "read": "anonymous"},
                    {"component": "saas-fabric", "role": "runtime", "repository": RUNTIME,
                     "registered": true, "read": "credential"},
                ]},
                {"host": "quay.io", "registered": false, "installed": false, "deployment": false, "images": [
                    {"component": "tools", "role": "cli", "repository": TOOLS,
                     "registered": false, "read": "notRead"},
                ]},
            ],
        })
    );
}

#[tokio::test]
async fn the_listing_itself_reads_no_desired_state() {
    let (plane, desired_state) = managed().await;
    desired_state.disconnect().await;

    let body = read(&plane, "/api/integrations/registries").await;

    // What was there is unchanged, and nothing of the platform is in it.
    assert_eq!(body["registries"][0]["host"], "ghcr.io");
    assert_eq!(body["deployment"], Value::Null);
    assert!(body.get("componentReads").is_none(), "{body}");
}

#[tokio::test]
async fn a_refused_credential_is_not_called_presented() {
    let (_, runtime) = ghcr_reads(Held::CredentialRefused).await;

    assert_eq!(runtime["read"], "credentialRefused");
}

#[tokio::test]
async fn a_registry_recorded_and_not_restored_reads_nothing() {
    let (host, runtime) = ghcr_reads(Held::Recorded).await;

    assert_eq!(host["registered"], true);
    assert_eq!(host["installed"], false);
    assert_eq!(runtime["read"], "notRead");
    assert_eq!(host["images"][0]["read"], "notRead");
}

#[tokio::test]
async fn a_deployment_managing_no_platform_says_so() {
    let plane = control_plane_with_registries(registries_holding(&[RUNTIME], false).await);

    let body = read(&plane, "/api/integrations/registries/reads").await;

    assert_eq!(body, json!({"state": "notManaged"}));
}

#[tokio::test]
async fn desired_state_that_cannot_be_read_is_said_with_its_code() {
    let (plane, desired_state) = managed().await;
    desired_state.disconnect().await;

    let body = read(&plane, "/api/integrations/registries/reads").await;

    assert_eq!(
        body,
        json!({"state": "unavailable", "code": "platform_not_managed"})
    );
}
