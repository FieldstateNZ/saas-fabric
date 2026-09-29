//! A described component (ADR 0026 section 9), read through the real router.
//!
//! The rule itself is proven against fakes in `fabric-platform-management`.
//! What these pin is the last step before an operator: that every answer the
//! rule gives reaches `GET /api/platform` as its own state, that `invalid`
//! carries its reason's code and nothing else carries one, and that the
//! rollback listing names the commit a described release was built from and
//! nothing an operator could send back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;

use axum::body::Body;
use fabric_platform_management::{Attached, AttachedDescriptor, Registry};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::described_registry::{attached, described_at, DescribedRegistry};
use support::platform_fixture::platform_binding_over;
use support::{as_operator, control_plane_with_platform, json, send};

/// The commit a coherent release is built from.
const COMMIT: &str = "5320432aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A commit only part of one release was built from.
const OTHER: &str = "46f88b0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A GET as an authenticated operator.
fn get(path: &str) -> Request<Body> {
    as_operator("GET", path)
        .body(Body::empty())
        .expect("the request must build")
}

/// What `path` answers, for an environment running `desired` of a described
/// component whose versions are in `registry`.
async fn read(registry: DescribedRegistry, desired: &str, path: &str) -> Value {
    let registry = Arc::new(registry) as Arc<dyn Registry>;
    let (platform, _fake) = platform_binding_over(registry, vec![described_at(desired)]).await;
    let plane = control_plane_with_platform(platform);

    let response = send(&plane.router, get(path)).await;

    assert_eq!(response.status(), StatusCode::OK, "{path}");
    json(response).await
}

/// `version`'s component descriptor, attached as a version of the family
/// this build does not read.
fn of_a_later_version(version: &str) -> Attached {
    match attached(version, COMMIT) {
        Attached::One(descriptor) => Attached::One(AttachedDescriptor {
            artifact_type: "application/vnd.saas-fabric.component.v2".to_owned(),
            ..descriptor
        }),
        other => panic!("the fixture attaches one component descriptor, not {other:?}"),
    }
}

#[tokio::test]
async fn every_answer_the_rule_gives_reaches_the_console_as_its_own_state() {
    // One version of each answer, above the one the environment runs. The
    // console words each for itself, so each has to arrive as itself: an
    // undescribed version is not still publishing, and an invalid one is not
    // built more than once.
    let registry = DescribedRegistry::default();

    // No component descriptor attached.
    registry.publish_images("0.3.0-preview.3", |_| COMMIT.to_owned());

    // More than one attached, never chosen between.
    registry.publish_images("0.3.0-preview.4", |_| COMMIT.to_owned());
    registry.describe("0.3.0-preview.4", Attached::Several { digests: Vec::new() });

    // One of a version this build does not read.
    registry.publish_images("0.3.0-preview.5", |_| COMMIT.to_owned());
    registry.describe("0.3.0-preview.5", of_a_later_version("0.3.0-preview.5"));

    // The control plane's image built from another commit.
    registry.publish_images("0.3.0-preview.6", |role| {
        if role == "controlPlane" { OTHER } else { COMMIT }.to_owned()
    });
    registry.describe("0.3.0-preview.6", attached("0.3.0-preview.6", COMMIT));

    // Complete: what the environment would advance to.
    registry.publish("0.3.0-preview.7", COMMIT);

    // Newer, and naming a control-plane image that was never pushed.
    registry.publish_image("0.3.0-preview.8", "runtime", COMMIT);
    registry.describe("0.3.0-preview.8", attached("0.3.0-preview.8", COMMIT));

    let body = read(registry, "0.3.0-preview.2", "/api/platform").await;
    let component = &body["components"][0];

    assert_eq!(component["component"], "saas-fabric");
    // Images, found through what the component says it is: a rollback
    // restores the same exact bytes, so the console is told `oci`.
    assert_eq!(component["artifact"], "oci");
    assert_eq!(component["newer"], "0.3.0-preview.7");

    // Exactly these rows: a `reason` on any row but an invalid one, or a
    // detail beside a reason's code, fails this as surely as a missing row.
    assert_eq!(
        component["diagnostics"],
        json!([
            { "version": "0.3.0-preview.3", "state": "undescribed" },
            { "version": "0.3.0-preview.6", "state": "incoherent" },
            { "version": "0.3.0-preview.8", "state": "invalid", "reason": "missingImage" },
            { "version": "0.3.0-preview.5", "state": "invalid", "reason": "unsupportedVersion", "found": "v2" },
            { "version": "0.3.0-preview.4", "state": "invalid", "reason": "several" },
        ])
    );
}

#[tokio::test]
async fn a_described_component_with_nothing_attached_is_not_offered_as_newer() {
    // The case every release of Fabric's own component is in until its
    // publishing job lands: tagged, complete as images, and undescribed. The
    // console says so rather than offering it.
    let registry = DescribedRegistry::default();
    registry.publish_images("0.3.0-preview.3", |_| COMMIT.to_owned());

    let body = read(registry, "0.3.0-preview.2", "/api/platform").await;
    let component = &body["components"][0];

    assert_eq!(component["newer"], Value::Null);
    assert_eq!(component["desiredState"], "current");
    assert_eq!(
        component["diagnostics"],
        json!([{ "version": "0.3.0-preview.3", "state": "undescribed" }])
    );
}

#[tokio::test]
async fn a_described_rollback_candidate_names_its_commit_and_no_digest() {
    // Only a complete release is offered, and it is offered as a version and
    // the commit it was built from -- under `source_revision`, the spelling
    // the console's rollback picker already reads for an image release. The component descriptor's digest is the
    // commit message's to name; like an image digest, it is nothing the
    // console should hold or could send back.
    let registry = DescribedRegistry::default();
    registry.publish("0.3.0-preview.1", COMMIT);
    registry.publish_images("0.3.0-preview.2", |_| COMMIT.to_owned());

    let body = read(
        registry,
        "0.3.0-preview.3",
        "/api/platform/components/saas-fabric/versions",
    )
    .await;

    assert_eq!(
        body,
        json!({
            "versions": [{ "version": "0.3.0-preview.1", "source_revision": COMMIT }],
            "more": false,
        })
    );
}
