//! Selecting a component version through `POST /api/catalogue` (ADR 0026
//! section 7), against a fake registry holding component descriptors
//! `fabric-component` rendered and a registry service holding which
//! repositories are registered.
//!
//! The rule itself is proven in `fabric-platform-management`, and the
//! catalogue's pure half in `fabric-client-model`. What these pin is the
//! path between them: every row of the selection table, each refusal with
//! its status and code, that a refusal the catalogue or the registry
//! records can decide costs no registry read, the deadline, that a save
//! cannot carry what the server resolves, that a save landing while a
//! version resolves wins the revision, that a publish freezes it, and what
//! the audit trail records.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Once, PoisonError};
use std::time::Duration;

use axum::body::Body;
use fabric_control_plane::{
    RegistryRecord, RegistryService, RegistryStore, RegistryStoreError, ResolutionParts,
};
use fabric_platform_management::{Attached, Registry, RegistryError};
use http::{header, StatusCode};
use serde_json::{json, Value};
use support::selection::{
    descriptor_digest, image_digest, registries_held, registries_holding, registries_over, Held,
    SelectionRegistry, DECLARED_FIELD, PRIMARY, SIBLING, TITLE,
};
use support::{as_operator, control_plane_with_resolution, send, TestControlPlane, FIXED_CLOCK_UNIX_SECONDS};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// The version every test selects first.
const VERSION: &str = "1.4.0";

/// A later version, for re-resolving.
const NEXT: &str = "1.5.0";

/// The commit a coherent release is built from.
const COMMIT: &str = "5320432aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A plane whose selections resolve through `registry`, holding
/// `registered` registered, with an application `analytics` saved: a
/// container component `reports` that the `reporting` feature names, and a
/// capability `identity`.
struct Harness {
    plane: TestControlPlane,
    registry: Arc<SelectionRegistry>,
    revision: String,
}

async fn harness_with(registry: SelectionRegistry, registered: &[&str], budget: Duration) -> Harness {
    harness_over(registry, registries_holding(registered, false).await, budget).await
}

/// As [`harness_with`], with the registry service `registries`.
async fn harness_over(
    registry: SelectionRegistry,
    registries: Arc<RegistryService>,
    budget: Duration,
) -> Harness {
    let registry = Arc::new(registry);
    let plane = control_plane_with_resolution(
        registries,
        ResolutionParts {
            registry: Arc::clone(&registry) as Arc<dyn Registry>,
            budget,
        },
        None,
    );
    let created = command(
        &plane,
        None,
        json!({"action": "createApplication", "id": "analytics", "name": "Analytics"}),
    )
    .await;
    let saved = command(
        &plane,
        created["revision"].as_str(),
        json!({"action": "saveApplication", "id": "analytics", "definition": definition(&authored())}),
    )
    .await;
    let revision = saved["revision"].as_str().unwrap().to_owned();
    Harness {
        plane,
        registry,
        revision,
    }
}

/// A harness whose registry holds a whole release of `VERSION` and `NEXT`,
/// with both repositories registered.
async fn harness() -> Harness {
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    registry.publish(NEXT, COMMIT);
    harness_with(registry, &[PRIMARY, SIBLING], Duration::from_secs(8)).await
}

/// The components as a save names them before anything is selected.
fn authored() -> Value {
    json!([
        {"kind": "container", "id": "reports", "name": "Reports", "reference": "registry.example.com/reports",
         "version": "1.0.0", "required": false, "policy": "automatic"},
        {"kind": "capability", "id": "identity", "name": "Identity", "reference": "Identity",
         "version": "", "required": true, "policy": "manual"},
    ])
}

/// An application draft holding `components`.
fn definition(components: &Value) -> Value {
    json!({"name": "Analytics", "description": "Reports", "domain": "{client}.example.com",
        "components": components,
        "features": [{"id": "reporting", "name": "Reporting", "description": "", "implementedBy": ["reports"]}],
        "plans": [{"id": "standard", "name": "Standard", "description": "", "features": ["reporting"],
                   "configuration": {}}],
        "fields": [{"key": "team", "label": "Team", "kind": "text", "required": true, "default": null,
                    "options": [], "description": ""}],
        "navigation": []})
}

/// Sends a catalogue command at `revision` — `If-None-Match: *` for none —
/// and answers its status and body.
async fn send_command(
    plane: &TestControlPlane,
    revision: Option<&str>,
    body: Value,
) -> (StatusCode, Value, bool) {
    let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
    let builder = match revision {
        Some(revision) => builder.header("if-match", format!("\"{revision}\"")),
        None => builder.header("if-none-match", "*"),
    };
    let response = send(&plane.router, builder.body(Body::from(body.to_string())).unwrap()).await;
    let status = response.status();
    let retry_after = response.headers().contains_key(header::RETRY_AFTER);
    (status, support::json(response).await, retry_after)
}

/// A command that must succeed.
async fn command(plane: &TestControlPlane, revision: Option<&str>, body: Value) -> Value {
    let (status, body, _) = send_command(plane, revision, body).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Selects `version` from `repository` for `component` of `analytics`.
async fn select(
    harness: &Harness,
    revision: &str,
    component: &str,
    repository: &str,
    version: &str,
) -> (StatusCode, Value, bool) {
    send_command(
        &harness.plane,
        Some(revision),
        json!({"action": "selectComponentVersion", "id": "analytics", "component": component,
               "repository": repository, "version": version}),
    )
    .await
}

/// The draft component `id` of `analytics` in a stored catalogue.
fn draft_component<'a>(stored: &'a Value, id: &str) -> &'a Value {
    stored["catalogue"]["applications"][0]["draft"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["id"] == id)
        .unwrap_or_else(|| panic!("no component {id} in {stored}"))
}

/// The current catalogue.
async fn catalogue(plane: &TestControlPlane) -> Value {
    let response = send(
        &plane.router,
        as_operator("GET", "/api/catalogue").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    support::json(response).await
}

/// Asserts a refusal's status and code, and that nothing was written.
async fn refused(
    harness: &Harness,
    answer: (StatusCode, Value, bool),
    status: StatusCode,
    code: &str,
) -> Value {
    let (got, body, _) = answer;
    assert_eq!(got, status, "{body}");
    assert_eq!(body["error"]["code"], code, "{body}");
    assert_eq!(
        catalogue(&harness.plane).await["revision"].as_str(),
        Some(harness.revision.as_str()),
        "a refused selection writes nothing"
    );
    body
}

// ---------------------------------------------------------------------------
// The selection table
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_id_that_names_nothing_creates_a_described_component_named_by_its_title() {
    let harness = harness().await;

    let (status, stored, _) = select(&harness, &harness.revision, "insights", PRIMARY, VERSION).await;

    assert_eq!(status, StatusCode::OK, "{stored}");
    let created = draft_component(&stored, "insights");
    assert_eq!(created["kind"], "described");
    assert_eq!(created["name"], TITLE);
    assert_eq!(created["reference"], PRIMARY);
    assert_eq!(created["version"], VERSION);
    assert_eq!(created["required"], false);
    assert_eq!(created["policy"], "manual");

    let resolution = &created["resolution"];
    assert_eq!(resolution["repository"], PRIMARY);
    assert_eq!(resolution["version"], VERSION);
    assert_eq!(resolution["primaryDigest"], image_digest(VERSION, "api"));
    assert_eq!(resolution["descriptorDigest"], descriptor_digest(VERSION));
    assert_eq!(resolution["revision"], COMMIT);
    assert_eq!(resolution["resolvedAt"], FIXED_CLOCK_UNIX_SECONDS);
    assert_eq!(resolution["descriptor"]["apiVersion"], "fabric.fieldstate.nz/v1");
    assert_eq!(resolution["descriptor"]["kind"], "Component");
    assert_eq!(
        resolution["descriptor"]["spec"]["fields"][0]["key"],
        DECLARED_FIELD
    );

    // The write is the catalogue's: stored, at a new revision, with its
    // activity entry, and read back the same.
    assert_ne!(stored["revision"], harness.revision.as_str());
    let activity = stored["catalogue"]["activity"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(activity["action"], "Component version selected");
    assert_eq!(activity["resource"], "analytics/insights");
    assert_eq!(catalogue(&harness.plane).await, stored);
}

#[tokio::test]
async fn a_container_component_is_converted_in_place_and_keeps_what_the_operator_decided() {
    let harness = harness().await;

    let (status, stored, _) = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;

    assert_eq!(status, StatusCode::OK, "{stored}");
    let converted = draft_component(&stored, "reports");
    assert_eq!(converted["kind"], "described");
    assert_eq!(
        converted["name"], "Reports",
        "its name, not the descriptor's title"
    );
    assert_eq!(converted["policy"], "automatic");
    assert_eq!(converted["required"], false);
    assert_eq!(converted["reference"], PRIMARY);
    assert_eq!(converted["resolution"]["version"], VERSION);
    // The feature that names it, and the plan that grants that, are kept.
    let draft = &stored["catalogue"]["applications"][0]["draft"];
    assert_eq!(draft["features"][0]["implementedBy"], json!(["reports"]));
    assert_eq!(draft["plans"][0]["features"], json!(["reporting"]));
}

#[tokio::test]
async fn a_described_component_is_re_resolved_and_the_same_one_again_is_already_selected() {
    let harness = harness().await;
    let (_, first, _) = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;

    let (status, next, _) = select(
        &harness,
        first["revision"].as_str().unwrap(),
        "reports",
        PRIMARY,
        NEXT,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{next}");
    let resolved = draft_component(&next, "reports");
    assert_eq!(resolved["name"], "Reports");
    assert_eq!(resolved["policy"], "automatic");
    assert_eq!(resolved["version"], NEXT);
    assert_eq!(
        resolved["resolution"]["descriptorDigest"],
        descriptor_digest(NEXT)
    );

    // The version it already records: nothing is written, so a timestamp
    // never becomes a change.
    let at = next["revision"].as_str().unwrap();
    let (status, body, _) = select(&harness, at, "reports", PRIMARY, NEXT).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "component_version_already_selected");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains(&descriptor_digest(NEXT)));
    assert_eq!(catalogue(&harness.plane).await["revision"].as_str(), Some(at));
}

#[tokio::test]
async fn a_capability_is_refused_before_any_registry_is_asked() {
    let harness = harness().await;

    let answer = select(&harness, &harness.revision, "identity", PRIMARY, VERSION).await;

    refused(
        &harness,
        answer,
        StatusCode::UNPROCESSABLE_ENTITY,
        "capability_not_selectable",
    )
    .await;
    assert_eq!(harness.registry.reads(), 0);
}

#[tokio::test]
async fn an_unknown_application_is_refused_before_any_registry_is_asked() {
    let harness = harness().await;

    let answer = send_command(
        &harness.plane,
        Some(&harness.revision),
        json!({"action": "selectComponentVersion", "id": "nothing", "component": "reports",
               "repository": PRIMARY, "version": VERSION}),
    )
    .await;

    refused(&harness, answer, StatusCode::BAD_REQUEST, "invalid_request").await;
    assert_eq!(harness.registry.reads(), 0);
}

// ---------------------------------------------------------------------------
// What the rule and the registries refuse
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_repository_not_registered_is_refused_before_any_registry_is_asked() {
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    let harness = harness_with(registry, &[SIBLING], Duration::from_secs(8)).await;

    let answer = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;

    let body = refused(
        &harness,
        answer,
        StatusCode::UNPROCESSABLE_ENTITY,
        "repository_not_registered",
    )
    .await;
    assert!(
        body["error"]["message"].as_str().unwrap().contains(PRIMARY),
        "{body}"
    );
    assert_eq!(harness.registry.reads(), 0);
}

#[tokio::test]
async fn a_tag_that_does_not_resolve_is_not_found() {
    let harness = harness().await;

    let answer = select(&harness, &harness.revision, "reports", PRIMARY, "2.0.0").await;

    let body = refused(
        &harness,
        answer,
        StatusCode::UNPROCESSABLE_ENTITY,
        "component_version_not_found",
    )
    .await;
    assert!(body["error"].get("answer").is_none(), "{body}");
}

/// A case of what the registry holds: its name, how to arrange it, the
/// answer, and the reason.
type Case = (
    &'static str,
    fn(&SelectionRegistry),
    &'static str,
    Option<&'static str>,
);

#[tokio::test]
async fn every_answer_that_is_not_a_release_unit_is_refused_with_the_answer_named() {
    // (what the registry holds, answer, reason)
    let cases: [Case; 4] = [
        (
            "no component descriptor attached",
            |registry| registry.publish_images(VERSION, COMMIT),
            "undescribed",
            None,
        ),
        (
            "one image built from another commit",
            |registry| {
                registry.publish(VERSION, COMMIT);
                registry.publish_image(VERSION, "web", "46f88b0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
            },
            "incoherent",
            None,
        ),
        (
            "more than one component descriptor attached",
            |registry| {
                registry.publish_images(VERSION, COMMIT);
                registry.describe(VERSION, Attached::Several { digests: Vec::new() });
            },
            "invalid",
            Some("several"),
        ),
        (
            "a descriptor naming another version",
            |registry| {
                registry.publish_images(VERSION, COMMIT);
                registry.describe(
                    VERSION,
                    support::selection::attached(VERSION, support::selection::spec(NEXT), COMMIT),
                );
            },
            "invalid",
            Some("wrongVersion"),
        ),
    ];

    for (case, arrange, answer, reason) in cases {
        let registry = SelectionRegistry::default();
        arrange(&registry);
        let harness = harness_with(registry, &[PRIMARY, SIBLING], Duration::from_secs(8)).await;

        let got = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;

        let body = refused(
            &harness,
            got,
            StatusCode::UNPROCESSABLE_ENTITY,
            "component_version_unusable",
        )
        .await;
        assert_eq!(body["error"]["answer"], answer, "{case}: {body}");
        assert_eq!(body["error"]["reason"].as_str(), reason, "{case}: {body}");
    }
}

#[tokio::test]
async fn a_sibling_repository_not_registered_makes_the_version_invalid() {
    // The primary is registered, the other image's repository is not: the
    // descriptor may not make Fabric read a repository nobody chose.
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    let harness = harness_with(registry, &[PRIMARY], Duration::from_secs(8)).await;

    let got = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;

    let body = refused(
        &harness,
        got,
        StatusCode::UNPROCESSABLE_ENTITY,
        "component_version_unusable",
    )
    .await;
    assert_eq!(body["error"]["answer"], "invalid");
    assert_eq!(body["error"]["reason"], "notRegistered");
    assert!(
        body["error"]["message"].as_str().unwrap().contains(SIBLING),
        "{body}"
    );
}

#[tokio::test]
async fn a_registry_that_cannot_be_asked_or_refuses_is_the_registrys_own_error() {
    let cases = [
        (
            RegistryError::Unavailable {
                detail: "ghcr.io answered 503".to_owned(),
            },
            StatusCode::SERVICE_UNAVAILABLE,
            "registry_unavailable",
            true,
        ),
        (
            RegistryError::Denied {
                detail: "the realm refused this registry's credential".to_owned(),
            },
            StatusCode::BAD_GATEWAY,
            "registry_refused",
            false,
        ),
        (
            RegistryError::Refused {
                detail: "no registry is registered for ghcr.io".to_owned(),
            },
            StatusCode::BAD_GATEWAY,
            "registry_refused",
            false,
        ),
    ];

    for (error, status, code, retry) in cases {
        let registry = SelectionRegistry::default();
        registry.publish(VERSION, COMMIT);
        registry.fail_with(error);
        let harness = harness_with(registry, &[PRIMARY, SIBLING], Duration::from_secs(8)).await;

        let got = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;
        let retry_after = got.2;

        let body = refused(&harness, got, status, code).await;
        assert_eq!(retry_after, retry, "{code}: {body}");
        assert!(
            body["error"].get("answer").is_none(),
            "an error is never an answer: {body}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_resolution_past_its_budget_is_unavailable_and_abandons_every_read() {
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    registry.hang();
    let harness = harness_with(registry, &[PRIMARY, SIBLING], Duration::from_secs(3)).await;

    let got = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;
    let retry_after = got.2;

    let body = refused(
        &harness,
        got,
        StatusCode::SERVICE_UNAVAILABLE,
        "registry_unavailable",
    )
    .await;
    assert!(retry_after);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("3-second resolution budget"),
        "{body}"
    );
    assert_eq!(harness.registry.reads(), 1, "the first read hung");
    assert_eq!(harness.registry.in_flight(), 0, "nothing is left reading");
    assert_eq!(harness.registry.abandoned(), 1);
}

#[tokio::test]
async fn a_registry_recorded_and_not_read_through_yet_is_unavailable_before_any_read() {
    // Recorded, and not restored since the last start: no client reads it,
    // so the router would answer its host as if the registry had refused.
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    let registries = registries_held(&[PRIMARY, SIBLING], false, Held::Recorded).await;
    let harness = harness_over(registry, registries, Duration::from_secs(8)).await;

    let got = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;
    let retry_after = got.2;

    let body = refused(
        &harness,
        got,
        StatusCode::SERVICE_UNAVAILABLE,
        "registry_unavailable",
    )
    .await;
    assert!(retry_after);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not being read through yet"),
        "{body}"
    );
    assert_eq!(harness.registry.reads(), 0);
}

/// A registry store whose every read hangs.
struct HangingStore;

#[async_trait::async_trait]
impl RegistryStore for HangingStore {
    async fn load(&self) -> Result<Vec<RegistryRecord>, RegistryStoreError> {
        std::future::pending().await
    }

    async fn save(&self, _records: &[RegistryRecord]) -> Result<(), RegistryStoreError> {
        std::future::pending().await
    }
}

#[tokio::test(start_paused = true)]
async fn the_budget_bounds_the_registry_store_read_too() {
    // Startup's sum counts a Git read, the budget and a Git write: the store
    // read between them must be inside the budget, or the sum lies.
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    let registries = registries_over(Arc::new(HangingStore));
    let harness = harness_over(registry, registries, Duration::from_secs(3)).await;

    let got = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;

    let body = refused(
        &harness,
        got,
        StatusCode::SERVICE_UNAVAILABLE,
        "registry_unavailable",
    )
    .await;
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("3-second resolution budget"),
        "{body}"
    );
    assert_eq!(harness.registry.reads(), 0);
}

#[tokio::test]
async fn a_selection_states_the_revision_it_edits() {
    let harness = harness().await;

    let stale = select(&harness, "0000000", "reports", PRIMARY, VERSION).await;
    refused(&harness, stale, StatusCode::CONFLICT, "revision_conflict").await;

    let unstated = send(
        &harness.plane.router,
        as_operator("POST", "/api/catalogue")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"action": "selectComponentVersion", "id": "analytics", "component": "reports",
                       "repository": PRIMARY, "version": VERSION})
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(unstated.status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(
        harness.registry.reads(),
        0,
        "a stale or unstated revision asks no registry"
    );
}

#[tokio::test]
async fn a_selection_body_carrying_a_digest_is_refused() {
    let harness = harness().await;

    let got = send_command(
        &harness.plane,
        Some(&harness.revision),
        json!({"action": "selectComponentVersion", "id": "analytics", "component": "reports",
               "repository": PRIMARY, "version": VERSION, "digest": image_digest(VERSION, "api")}),
    )
    .await;

    assert_eq!(got.0, StatusCode::BAD_REQUEST, "{}", got.1);
    assert_eq!(harness.registry.reads(), 0);
}

// ---------------------------------------------------------------------------
// Saving, publishing, and a client's copy
// ---------------------------------------------------------------------------

/// A save naming `reports` as described, with `extra` merged into it.
fn described_save(extra: &Value) -> Value {
    let mut described = json!({"kind": "described", "id": "reports", "name": "Reports",
                                "required": false, "policy": "automatic"});
    for (key, value) in extra.as_object().unwrap() {
        described[key] = value.clone();
    }
    let mut components = authored();
    components[0] = described;
    json!({"action": "saveApplication", "id": "analytics", "definition": definition(&components)})
}

#[tokio::test]
async fn a_save_cannot_carry_what_the_server_resolves_and_keeps_the_stored_resolution() {
    let harness = harness().await;
    let (_, selected, _) = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;
    let at = selected["revision"].as_str().unwrap();
    let resolution = draft_component(&selected, "reports")["resolution"].clone();

    for carried in [
        json!({"resolution": resolution}),
        json!({"reference": "ghcr.io/acme/elsewhere"}),
        json!({"version": NEXT}),
        json!({"primaryDigest": image_digest(NEXT, "api")}),
        json!({"descriptorDigest": descriptor_digest(NEXT)}),
        json!({"fields": [{"key": "forged"}]}),
    ] {
        let (status, body, _) = send_command(&harness.plane, Some(at), described_save(&carried)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{carried}: {body}");
        assert_eq!(body["error"]["code"], "invalid_request", "{carried}: {body}");
    }
    assert_eq!(catalogue(&harness.plane).await["revision"].as_str(), Some(at));

    // Omitted, it is kept: the operator's decisions change, the resolution
    // does not.
    let saved = command(
        &harness.plane,
        Some(at),
        described_save(&json!({"name": "Renamed", "required": true, "policy": "manual"})),
    )
    .await;
    let kept = draft_component(&saved, "reports");
    assert_eq!(kept["name"], "Renamed");
    assert_eq!(kept["required"], true);
    assert_eq!(kept["policy"], "manual");
    assert_eq!(kept["resolution"], resolution);
    assert_eq!(kept["reference"], PRIMARY);
    assert_eq!(kept["version"], VERSION);
}

#[tokio::test]
async fn a_save_landing_while_a_version_resolves_wins_and_the_selection_is_a_lost_race() {
    // The selection reads the catalogue at its revision, then resolves, then
    // writes at that same revision. A save that lands between the read and
    // the write moves the revision, and the write must lose to it: the
    // resolution, however complete, may not overwrite what the operator
    // saved in the meantime. The registry's first read is held open so the
    // save can land in that window, deterministically and without a clock.
    let harness = Arc::new(harness().await);
    let held = harness.registry.hold_first_read();

    let racing = {
        let harness = Arc::clone(&harness);
        tokio::spawn(async move { select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await })
    };
    held.reached().await;

    // The selection has read the catalogue and asked the registry, and has
    // written nothing yet.
    assert_eq!(harness.registry.reads(), 1, "the held read is the first");
    assert_eq!(
        catalogue(&harness.plane).await["revision"].as_str(),
        Some(harness.revision.as_str()),
        "nothing is written before the resolution answers"
    );

    // Another operator request saves a valid edit at the revision the
    // selection is editing, and wins it.
    let mut edited = authored();
    edited[0]["name"] = json!("Monthly Reports");
    let saved = command(
        &harness.plane,
        Some(&harness.revision),
        json!({"action": "saveApplication", "id": "analytics", "definition": definition(&edited)}),
    )
    .await;
    let winning = saved["revision"].as_str().unwrap().to_owned();
    assert_ne!(winning, harness.revision);

    held.release();
    let (status, body, retry_after) = tokio::time::timeout(Duration::from_secs(10), racing)
        .await
        .expect("the released selection must answer")
        .expect("the selection task must not panic");

    // The resolution completed and the write lost: the catalogue's own
    // conflict, as every lost catalogue race is answered.
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "revision_conflict", "{body}");
    assert_eq!(
        body["error"]["message"],
        "the catalogue changed since it was read; re-read it and apply the change again",
        "{body}"
    );
    assert!(body["error"].get("answer").is_none(), "a lost race is no answer of the rule: {body}");
    assert!(!retry_after, "re-reading, not waiting, is the remedy");
    assert!(
        harness.registry.reads() > 1,
        "the resolution resumed past the held read and ran to its answer"
    );
    assert_eq!(harness.registry.in_flight(), 0);

    // The winning edit stands, untouched by the selection: the revision is
    // the save's, the component is still the authored container, and no
    // resolution or selection record was written over it.
    let stored = catalogue(&harness.plane).await;
    assert_eq!(stored["revision"].as_str(), Some(winning.as_str()));
    let kept = draft_component(&stored, "reports");
    assert_eq!(kept["kind"], "container");
    assert_eq!(kept["name"], "Monthly Reports");
    assert_eq!(kept["reference"], "registry.example.com/reports");
    assert_eq!(kept["version"], "1.0.0");
    assert!(kept.get("resolution").is_none(), "{kept}");
    let activity = stored["catalogue"]["activity"].as_array().unwrap();
    assert!(
        activity
            .iter()
            .all(|event| event["action"] != "Component version selected"),
        "{activity:?}"
    );
}

#[tokio::test]
async fn a_publish_freezes_the_resolution_and_a_client_keeps_its_copy() {
    let harness = harness().await;
    let (_, selected, _) = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;
    let resolution = draft_component(&selected, "reports")["resolution"].clone();
    let read = harness.registry.reads();

    let published = command(
        &harness.plane,
        selected["revision"].as_str(),
        json!({"action": "publishApplication", "id": "analytics", "note": "First described release"}),
    )
    .await;
    assert_eq!(harness.registry.reads(), read, "publishing calls no registry");
    let release = &published["catalogue"]["applications"][0]["releases"][0]["definition"];
    let frozen = release["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["id"] == "reports")
        .unwrap();
    assert_eq!(frozen["resolution"], resolution);

    // A later selection moves the draft, not the release.
    let (_, moved, _) = select(
        &harness,
        published["revision"].as_str().unwrap(),
        "reports",
        PRIMARY,
        NEXT,
    )
    .await;
    let application = &moved["catalogue"]["applications"][0];
    assert_eq!(
        application["draft"]["components"][0]["resolution"]["version"],
        NEXT
    );
    assert_eq!(
        application["releases"][0]["definition"]["components"][0]["resolution"],
        resolution
    );

    // A client assigned the release copies it, resolution and all, and reads
    // it back from its own document.
    let client = json!({"id": "newco", "configuration": {
        "displayName": "Newco", "legalName": "Newco Ltd", "region": "NZ", "timezone": "Pacific/Auckland",
        "hosts": ["newco.example.com"], "configuration": {},
        "applications": [{"applicationId": "analytics", "version": 1, "planId": "standard",
                          "configuration": {"team": "Finance"}}]}});
    let created = send(
        &harness.plane.router,
        as_operator("POST", "/api/clients")
            .header("content-type", "application/json")
            .body(Body::from(client.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let product = send(
        &harness.plane.router,
        as_operator("GET", "/api/clients/newco/product")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let product = support::json(product).await;
    let granted = &product["resolved"][0]["components"];
    assert_eq!(granted[0]["id"], "reports", "{product}");
    assert_eq!(granted[0]["resolution"], resolution);
    assert_eq!(
        product["product"]["applications"][0]["release"]["definition"]["components"][0]["resolution"],
        resolution
    );
}

// ---------------------------------------------------------------------------
// The audit trail
// ---------------------------------------------------------------------------

/// Events captured on one thread: each one's fields by name.
type Captured = Arc<Mutex<Vec<BTreeMap<String, String>>>>;

thread_local! {
    /// This thread's captured events, while a test has opted in.
    static SINK: RefCell<Option<Captured>> = const { RefCell::new(None) };
}

/// Routes each event to the thread that opted in. Installed once, globally:
/// a scoped default races every other test's callsites over tracing's
/// interest cache.
struct CapturingLayer;

struct Fields(BTreeMap<String, String>);

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        SINK.with(|sink| {
            if let Some(events) = sink.borrow().as_ref() {
                let mut fields = Fields(BTreeMap::new());
                event.record(&mut fields);
                events
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(fields.0);
            }
        });
    }
}

/// Starts capturing this thread's events.
fn capture() -> Captured {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(CapturingLayer))
            .expect("this binary installs the only global subscriber");
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    SINK.with(|sink| *sink.borrow_mut() = Some(Arc::clone(&events)));
    events
}

/// The selection audit records captured.
fn selections(events: &Captured) -> Vec<BTreeMap<String, String>> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| {
            event.get("event").map(String::as_str) == Some("control_plane.audit.component_selected")
        })
        .cloned()
        .collect()
}

#[tokio::test]
async fn a_selection_and_a_refusal_are_each_audited_with_what_was_proven() {
    let events = capture();
    let registry = SelectionRegistry::default();
    registry.publish(VERSION, COMMIT);
    registry.publish_images(NEXT, COMMIT);
    registry.describe(NEXT, Attached::Several { digests: Vec::new() });
    let harness = harness_with(registry, &[PRIMARY, SIBLING], Duration::from_secs(8)).await;

    let (_, selected, _) = select(&harness, &harness.revision, "reports", PRIMARY, VERSION).await;
    let (status, _, _) = select(
        &harness,
        selected["revision"].as_str().unwrap(),
        "reports",
        PRIMARY,
        NEXT,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let audited = selections(&events);
    assert_eq!(audited.len(), 2, "{audited:?}");

    let written = &audited[0];
    assert_eq!(written["operation"], "select_component_version");
    assert_eq!(written["requested_by"], support::OPERATOR);
    assert_eq!(written["entry"], "analytics/reports");
    assert_eq!(written["outcome"], "selected");
    assert_eq!(written["repository"], PRIMARY);
    assert_eq!(written["version"], VERSION);
    assert_eq!(written["primary_digest"], image_digest(VERSION, "api"));
    assert_eq!(written["descriptor_digest"], descriptor_digest(VERSION));
    assert_eq!(
        written["event_id"], "10015",
        "the control plane's fifteenth success event"
    );

    let refusal = &audited[1];
    assert_eq!(refusal["outcome"], "component_version_unusable");
    assert_eq!(refusal["answer"], "invalid");
    assert_eq!(refusal["reason"], "several");
    assert_eq!(refusal["version"], NEXT);
    assert_eq!(
        refusal["primary_digest"], "",
        "a refusal records no digest it did not prove"
    );
    assert_eq!(refusal["descriptor_digest"], "");

    // The write is also a catalogue change, with the revision it made.
    let changed = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| {
            event.get("event").map(String::as_str) == Some("control_plane.audit.catalogue_changed")
        })
        .filter(|event| event.get("operation").map(String::as_str) == Some("select_component_version"))
        .count();
    assert_eq!(changed, 1);
}
