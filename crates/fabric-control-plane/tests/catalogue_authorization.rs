//! Anonymous catalogue *mutations* are refused, and refused before they
//! change anything.
//!
//! `product_workflows::catalogue_requires_auth_and_conditional_writes` shows
//! an anonymous `GET /api/catalogue` is refused. It does not show what
//! happens to an anonymous `POST /api/catalogue` that is otherwise entirely
//! valid — a well-formed command, the right precondition header, against a
//! catalogue in exactly the state the command expects. That is the request
//! an operator-extractor regression would let through, and a `401` from a
//! malformed body or a stale revision would prove nothing about it. So every
//! anonymous request here is one that, with an operator attached, succeeds —
//! and each test sends exactly that request again, authenticated, as its
//! positive control.
//!
//! "Changed nothing" is read back through the real router too: the whole
//! `GET /api/catalogue` body, revision included, before and after the
//! refused write.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    // As in `platform_publication.rs`: `allow-indexing-slicing-in-tests` in
    // `clippy.toml` only recognises indexing directly inside a
    // `#[tokio::test]` function, and `revision_of` and
    // `assert_unauthenticated` below navigate JSON one level down.
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use axum::Router;
use http::{Request, Response, StatusCode};
use serde_json::{json, Value};
use support::{as_operator, control_plane, json as body_of, send};

/// The precondition a catalogue command carries: `If-None-Match: *` for the
/// very first write, `If-Match` naming the current revision after that.
enum Precondition<'a> {
    FirstWrite,
    Revision(&'a str),
}

/// `POST /api/catalogue` as an operator.
async fn operator_command(
    router: &Router,
    precondition: &Precondition<'_>,
    command: &Value,
) -> Response<Body> {
    command_from(
        as_operator("POST", "/api/catalogue"),
        router,
        precondition,
        command,
    )
    .await
}

/// `POST /api/catalogue` with no operator at all — the same request as
/// [`operator_command`], built without the operator header rather than by
/// reaching into the harness (see `support::OPERATOR_HEADER`).
async fn anonymous_command(
    router: &Router,
    precondition: &Precondition<'_>,
    command: &Value,
) -> Response<Body> {
    let builder = Request::builder().method("POST").uri("/api/catalogue");
    command_from(builder, router, precondition, command).await
}

async fn command_from(
    builder: http::request::Builder,
    router: &Router,
    precondition: &Precondition<'_>,
    value: &Value,
) -> Response<Body> {
    let builder = builder.header("content-type", "application/json");
    let builder = match precondition {
        Precondition::FirstWrite => builder.header("if-none-match", "*"),
        Precondition::Revision(revision) => builder.header("if-match", format!("\"{revision}\"")),
    };
    send(router, builder.body(Body::from(value.to_string())).unwrap()).await
}

/// The whole `GET /api/catalogue` body — `catalogue` and `revision` — as an
/// operator. Asserts `200` first so a failure here reads as "the read
/// failed", not as a missing key further down.
async fn read_catalogue(router: &Router) -> Value {
    let response = send(
        router,
        as_operator("GET", "/api/catalogue").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    body_of(response).await
}

/// Asserts `401` and the extractor's own code — and only then reads the
/// body, so a wrong status is reported as such rather than as a JSON shape
/// that happened not to carry `error.code`.
async fn assert_unauthenticated(response: Response<Body>) {
    let status = response.status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "unauthenticated", "{body}");
}

/// A `200` catalogue-command response, read as JSON. Status first, for the
/// same reason as [`assert_unauthenticated`].
async fn assert_applied(response: Response<Body>) -> Value {
    let status = response.status();
    let body = body_of(response).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// A publishable draft: at least one plan, which is what `publishApplication`
/// insists on. Mirrors `product_workflows::definition`.
fn definition() -> Value {
    json!({"name":"Analytics", "description":"Reports", "domain":"{client}.example.com",
        "components":[{"id":"reports", "name":"Reports", "kind":"container", "reference":"registry.example.com/reports", "version":"1.0.0", "required":false,"policy":"manual"}],
        "features":[{"id":"reporting","name":"Reporting","description":"", "implementedBy":["reports"]}],
        "plans":[{"id":"standard","name":"Standard","description":"","features":["reporting"],"configuration":{"seats":"10"}}],
        "fields":[{"key":"team","label":"Team","kind":"text","required":true,"default":null,"options":[],"description":""}],
        "navigation":[{"label":"Reports","route":"/reports","feature":"reporting","permission":"reports.read"}]})
}

fn create_analytics() -> Value {
    json!({"action":"createApplication","id":"analytics","name":"Analytics"})
}

/// The current revision out of a `GET /api/catalogue` or command body, as
/// the string `If-Match` needs. Panics with the body if there is none, so the
/// test that depended on a prior write names what it actually read.
fn revision_of(body: &Value) -> String {
    body["revision"]
        .as_str()
        .unwrap_or_else(|| panic!("the catalogue must carry a revision here: {body}"))
        .to_owned()
}

#[tokio::test]
async fn an_anonymous_create_against_an_empty_catalogue_is_refused_and_writes_nothing() {
    let app = control_plane();

    // Nothing has written the catalogue yet: no applications, no revision —
    // exactly the state `If-None-Match: *` is for, so the request below is
    // valid in every respect but who sent it.
    let before = read_catalogue(&app.router).await;
    assert_eq!(before["catalogue"]["applications"], json!([]), "{before}");
    assert!(before["revision"].is_null(), "{before}");

    let refused = anonymous_command(&app.router, &Precondition::FirstWrite, &create_analytics()).await;
    assert_unauthenticated(refused).await;

    let after = read_catalogue(&app.router).await;
    assert_eq!(
        after, before,
        "a refused anonymous create must leave the catalogue exactly as it was"
    );

    // Positive control: the identical request, with an operator, is the
    // first write — so the `401` above was about the operator, not about
    // the body or the precondition.
    let applied = operator_command(&app.router, &Precondition::FirstWrite, &create_analytics()).await;
    let applied = assert_applied(applied).await;
    assert_eq!(
        applied["catalogue"]["applications"][0]["id"], "analytics",
        "{applied}"
    );
    assert!(applied["revision"].is_string(), "{applied}");

    let created = read_catalogue(&app.router).await;
    assert_eq!(
        created["catalogue"]["applications"][0]["id"], "analytics",
        "{created}"
    );
    assert_ne!(created, before);
}

#[tokio::test]
async fn an_anonymous_draft_save_with_the_current_revision_is_refused_and_writes_nothing() {
    let app = control_plane();

    // Seeded through the real router, as an operator would: create, then
    // give the draft a known description to change.
    let created = operator_command(&app.router, &Precondition::FirstWrite, &create_analytics()).await;
    let created = assert_applied(created).await;
    let saved = operator_command(
        &app.router,
        &Precondition::Revision(&revision_of(&created)),
        &json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    assert_applied(saved).await;

    let before = read_catalogue(&app.router).await;
    let revision = revision_of(&before);
    assert_eq!(
        before["catalogue"]["applications"][0]["draft"]["description"], "Reports",
        "{before}"
    );

    // The same draft with one field changed, carrying the revision the
    // catalogue is actually at. An operator sending this succeeds — see
    // below — so nothing but the missing operator can be what refuses it.
    let mut changed = definition();
    changed["description"] = json!("Reports, now anonymous");
    let save = json!({"action":"saveApplication","id":"analytics","definition":changed});

    let refused = anonymous_command(&app.router, &Precondition::Revision(&revision), &save).await;
    assert_unauthenticated(refused).await;

    let after = read_catalogue(&app.router).await;
    assert_eq!(
        after, before,
        "a refused anonymous save must leave the catalogue exactly as it was"
    );

    // Positive control: same body, same `If-Match`, with an operator.
    let applied = operator_command(&app.router, &Precondition::Revision(&revision), &save).await;
    let applied = assert_applied(applied).await;
    assert_eq!(
        applied["catalogue"]["applications"][0]["draft"]["description"], "Reports, now anonymous",
        "{applied}"
    );

    let edited = read_catalogue(&app.router).await;
    assert_eq!(
        edited["catalogue"]["applications"][0]["draft"]["description"], "Reports, now anonymous",
        "{edited}"
    );
    assert_ne!(
        revision_of(&edited),
        revision,
        "an applied save moves the revision"
    );
}

#[tokio::test]
async fn an_anonymous_publication_with_the_current_revision_is_refused_and_releases_nothing() {
    let app = control_plane();

    // A draft that *can* publish — `definition()` carries a plan — at the
    // revision its save produced.
    let created = operator_command(&app.router, &Precondition::FirstWrite, &create_analytics()).await;
    let created = assert_applied(created).await;
    let saved = operator_command(
        &app.router,
        &Precondition::Revision(&revision_of(&created)),
        &json!({"action":"saveApplication","id":"analytics","definition":definition()}),
    )
    .await;
    assert_applied(saved).await;

    let before = read_catalogue(&app.router).await;
    let revision = revision_of(&before);
    assert_eq!(
        before["catalogue"]["applications"][0]["releases"],
        json!([]),
        "nothing is published yet: {before}"
    );

    let publish = json!({"action":"publishApplication","id":"analytics","note":"Initial release"});

    let refused = anonymous_command(&app.router, &Precondition::Revision(&revision), &publish).await;
    assert_unauthenticated(refused).await;

    let after = read_catalogue(&app.router).await;
    assert_eq!(
        after, before,
        "a refused anonymous publication must leave the catalogue exactly as it was"
    );

    // Positive control: the identical publication, as an operator, releases
    // version 1.
    let applied = operator_command(&app.router, &Precondition::Revision(&revision), &publish).await;
    let applied = assert_applied(applied).await;
    assert_eq!(
        applied["catalogue"]["applications"][0]["releases"][0]["version"], 1,
        "{applied}"
    );

    let published = read_catalogue(&app.router).await;
    assert_eq!(
        published["catalogue"]["applications"][0]["releases"][0]["version"], 1,
        "{published}"
    );
    assert_eq!(
        published["catalogue"]["applications"][0]["releases"][0]["note"], "Initial release",
        "{published}"
    );
    assert_ne!(
        revision_of(&published),
        revision,
        "an applied publication moves the revision"
    );
}
