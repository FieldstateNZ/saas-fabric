//! `/api/platform/data-sources` through the real router (ADR 0023 part 1).
//!
//! One end-to-end sequence rather than one test per outcome: the outcomes
//! only mean something in relation to each other -- a "corrected" only
//! proves anything once a "declared" precedes it, and "unchanged" only
//! proves anything once a real edit has shown the fake *does* record a
//! write when one happens. `docs/delivery.md` calls this the mandatory
//! HTTP-surface test for this slice.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use axum::body::Body;
use fabric_core::{BindingRevision, DataSourceId};
use fabric_platform_management::{
    ConnectionName, ConnectorId, DataResidencyDocument, DataSourceCapabilitiesDocument,
    DataSourceDeclaration, DesiredRevision, FieldName, PlacementClassDocument, PoolSettingsDocument,
};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use support::platform_fixture::platform_binding;
use support::{as_operator, control_plane_with_platform, entity_tag, json as body_of, send};

const PATH: &str = "/api/platform/data-sources";
const ID: &str = "shared-postgres-nz-01";
const HA_ID: &str = "ha-postgres-nz-01";

/// A shared, named-connection declaration, with the discriminator and pool
/// size a test wants to vary -- every other field held fixed, since only
/// these two are what any step below changes.
fn declaration(discriminator: Option<&str>, max_connections: u32) -> Value {
    json!({
        "connector": "postgres-nz",
        "connection": {"kind": "named", "name": "shared"},
        "placement": "shared",
        "residency": {"region": "nz", "jurisdiction": "NZ"},
        "pool": {
            "maxConnections": max_connections,
            "idleTimeoutSeconds": 300,
            "acquireTimeoutSeconds": 5,
        },
        "capabilities": {"writable": true, "acceptsNewTenants": true},
        "discriminator": discriminator.map(|column| json!({"column": column})),
        "labels": {},
    })
}

/// A second data source, shaped to carry every field the shared/named one
/// above does not: a secret connection, a placement other than `shared`
/// (so no discriminator), no jurisdiction, and no labels.
fn declaration_high_availability() -> Value {
    json!({
        "connector": "postgres-nz",
        "connection": {"kind": "secret", "reference": "tenant/acme/data-primary"},
        "placement": "highAvailability",
        "residency": {"region": "nz", "jurisdiction": null},
        "pool": {
            "maxConnections": 10,
            "idleTimeoutSeconds": 300,
            "acquireTimeoutSeconds": 5,
        },
        "capabilities": {"writable": true, "acceptsNewTenants": false},
        "discriminator": null,
        "labels": {},
    })
}

/// A `PUT` as an authenticated operator, carrying one precondition header.
fn put(path: &str, precondition: (&str, &str), body: &Value) -> Request<Body> {
    as_operator("PUT", path)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(precondition.0, precondition.1)
        .body(Body::from(body.to_string()))
        .expect("the request must build")
}

/// A `PUT` with no precondition header at all.
fn put_without_precondition(path: &str, body: &Value) -> Request<Body> {
    as_operator("PUT", path)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("the request must build")
}

/// A `GET` as an authenticated operator.
fn get(path: &str) -> Request<Body> {
    as_operator("GET", path)
        .body(Body::empty())
        .expect("the request must build")
}

/// The rendered data source named `id` within a `dataSources` body,
/// regardless of where the list sorted it.
fn entry<'a>(body: &'a Value, id: &str) -> &'a Value {
    body["dataSources"]
        .as_array()
        .expect("dataSources must be an array")
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("no data source named {id} in {body}"))
}

/// A declaration this crate cannot itself produce -- two entries claiming
/// the same id -- for seeding the fake directly, the way a break-glass
/// hand edit on disk would reach it (ADR 0023 part 1).
fn duplicate_declaration(id: &str) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: fabric_platform_management::ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: Some(fabric_platform_management::Discriminator {
            column: FieldName::try_new("tenant_key").expect("a valid field name"),
        }),
        labels: BTreeMap::new(),
    }
}

#[tokio::test]
#[allow(
    clippy::too_many_lines,
    reason = "one sequence, not many tests: each step only proves something in relation \
              to the one before it, so splitting them would need the same setup repeated \
              and would let one step change without the others noticing"
)]
async fn declare_correct_conflict_refuse_and_leave_unchanged() {
    let (platform, fake) = platform_binding().await;
    let plane = control_plane_with_platform(platform);

    // 1. An environment with nothing declared yet already has a revision
    // and an ETag -- the late-bound binding tags even an absent document,
    // so there is always something to send back as `If-Match`.
    let response = send(&plane.router, get(PATH)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let empty_tag = entity_tag(&response);
    let body = body_of(response).await;
    assert_eq!(body["environment"], support::platform_fixture::ENVIRONMENT);
    assert_eq!(body["revision"], empty_tag);
    assert_eq!(body["dataSources"], json!([]));

    // 2. The first declaration this environment ever makes sends that tag
    // back as `If-Match`.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{ID}"),
            ("If-Match", &format!("\"{empty_tag}\"")),
            &declaration(Some("tenant_key"), 20),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a first declaration against the tag GET returned must be accepted"
    );
    let first_tag = entity_tag(&response);
    assert_ne!(first_tag, empty_tag, "declaring something must move the revision");
    let body = body_of(response).await;
    let shared = entry(&body, ID);
    assert_eq!(shared["revision"], 1);
    assert_eq!(shared["discriminator"]["column"], "tenant_key");
    assert_eq!(shared["residency"]["jurisdiction"], "NZ");
    assert_eq!(shared["labels"], json!({}));
    assert_eq!(shared["capabilities"]["acceptsNewTenants"], true);
    assert_eq!(shared["connection"], json!({"kind": "named", "name": "shared"}));
    assert_eq!(shared["placement"], "shared");

    // 3. A second data source, with every field the first does not carry:
    // a secret connection, a placement other than `shared` (so no
    // discriminator), no jurisdiction, and no labels. Declared against the
    // document's current revision -- the revision is per document, not per
    // id, so this is `first_tag` even though it names a different source.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{HA_ID}"),
            ("If-Match", &format!("\"{first_tag}\"")),
            &declaration_high_availability(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let second_declare_tag = entity_tag(&response);
    let body = body_of(response).await;
    let ha = entry(&body, HA_ID);
    assert_eq!(ha["placement"], "highAvailability");
    assert_eq!(ha["discriminator"], Value::Null);
    assert_eq!(ha["residency"]["jurisdiction"], Value::Null);
    assert_eq!(ha["labels"], json!({}));
    assert_eq!(
        ha["connection"],
        json!({"kind": "secret", "reference": "tenant/acme/data-primary"})
    );
    assert_eq!(ha["capabilities"]["acceptsNewTenants"], false);

    // 4. List finds both, with the same entity tag the last declaration
    // answered.
    let response = send(&plane.router, get(PATH)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(entity_tag(&response), second_declare_tag);
    let body = body_of(response).await;
    assert_eq!(entry(&body, ID)["id"], ID);
    assert_eq!(entry(&body, HA_ID)["id"], HA_ID);

    // 5. Correct the first, changing its pool size.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{ID}"),
            ("If-Match", &format!("\"{second_declare_tag}\"")),
            &declaration(Some("tenant_key"), 50),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a correction against the current revision must be accepted"
    );
    let corrected_tag = entity_tag(&response);
    assert_ne!(
        corrected_tag, second_declare_tag,
        "a written correction must move the revision"
    );
    let body = body_of(response).await;
    assert_eq!(entry(&body, ID)["revision"], 2);
    assert_eq!(entry(&body, ID)["pool"]["maxConnections"], 50);

    // 6. A further edit against the now-stale `first_tag` is refused, and
    // with the code the console keys its reload affordance on -- the same
    // one a stale catalogue write answers, not the platform's generic
    // `platform_state_moved`.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{ID}"),
            ("If-Match", &format!("\"{first_tag}\"")),
            &declaration(Some("tenant_key"), 99),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "a stale If-Match must be refused rather than silently applied over a lost edit"
    );
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_conflict");

    // 7. A stale If-Match is refused even when the declaration is
    // identical to what is already held. Content equal to what a fresh
    // read would find plans as "nothing to write", which must not let a
    // stale precondition slip through unchecked -- the caller still read
    // this before the correction in step 5 landed, and is owed the same
    // conflict as an edit would get.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{ID}"),
            ("If-Match", &format!("\"{first_tag}\"")),
            &declaration(Some("tenant_key"), 50),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "an identical declaration does not excuse a stale precondition"
    );
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_conflict");

    // 8. A shared data source with no discriminator column is refused,
    // with the structural 422 `invalid_data_source` mapping.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/shared-without-discriminator"),
            ("If-Match", &format!("\"{corrected_tag}\"")),
            &declaration(None, 20),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "invalid_data_source");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("an error body carries a message")
            .contains("discriminator"),
        "{body}"
    );

    // 9. No precondition header at all is refused -- a blind write is
    // last-writer-wins, which this route never accepts, including for the
    // very first declaration (step 2 above always has a tag to send).
    let response = send(
        &plane.router,
        put_without_precondition(&format!("{PATH}/{ID}"), &declaration(Some("tenant_key"), 50)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_required");

    // 10. `If-None-Match: *` alone is no longer accepted on this route: the
    // late-bound binding always has a tag for this environment, so there is
    // no state "only if absent" could mean here that `If-Match` cannot
    // already say -- carrying it without `If-Match` is refused the same as
    // sending no precondition at all.
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{ID}"),
            ("If-None-Match", "*"),
            &declaration(Some("tenant_key"), 50),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "revision_required");

    // 11. Resubmitting exactly what is held writes nothing.
    let writes_before = fake.writes();
    let response = send(
        &plane.router,
        put(
            &format!("{PATH}/{ID}"),
            ("If-Match", &format!("\"{corrected_tag}\"")),
            &declaration(Some("tenant_key"), 50),
        ),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "an unchanged declaration is not a failure"
    );
    assert_eq!(
        entity_tag(&response),
        corrected_tag,
        "nothing moved, so the revision must not either"
    );
    assert_eq!(
        fake.writes(),
        writes_before,
        "a declaration that matches what is held must not reach the write port at all"
    );

    // 12. A hand-broken document -- two entries claiming the same id -- is
    // not something this route's validation can reach; it can only be
    // seeded directly, the way a break-glass edit on disk would. Reading it
    // back answers the same `500` `desired_state_invalid` a broken client
    // document already does, not the generic platform mapping's `503`.
    fake.seed(
        DesiredRevision::new("hand-edited"),
        vec![
            duplicate_declaration("dup-source"),
            duplicate_declaration("dup-source"),
        ],
    );
    let response = send(&plane.router, get(PATH)).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_of(response).await;
    assert_eq!(body["error"]["code"], "desired_state_invalid");
    // Names the duplicate, matching `check_held`'s own wording exactly --
    // so this test cannot pass on some other refusal that happens to
    // share the same status and code (an adapter failure, say).
    assert_eq!(body["error"]["message"], "dup-source is declared more than once");
}
