//! The control-plane API's contract, over HTTP.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. Clippy's `allow-unwrap-in-tests` only covers
// `#[test]` functions, so an integration test file states it once here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::body::Body;
use fabric_client_model::{ClientId, OidcClientId, PkceMethod, RealmName, RedirectUri};
use fabric_control_plane::ClientRepository as _;
use fabric_reconciliation::testing::FakeIdentityProvider;
use fabric_reconciliation::IdentityProvider as _;
use http::{header, StatusCode};
use support::{as_operator, control_plane, control_plane_with_identity_provider, entity_tag, json, send};

/// The identity an operator would submit after adding a role.
fn identity_with_extra_role() -> Body {
    Body::from(
        serde_json::json!({
            "realm": "acme",
            "roles": ["Client Realm Administrator", "Client Realm User", "Invoicing Approver"],
            "clients": [{
                "id": "web",
                "type": "oidc",
                "pkce": "s256",
                "redirect": {
                    "strategy": "claimedHttps",
                    "uris": ["https://www.example.com/callback"],
                },
            }],
        })
        .to_string(),
    )
}

#[tokio::test]
async fn listing_clients_returns_the_desired_state_source() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients").body(Body::empty()).unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = json(response).await;
    assert_eq!(body["clients"][0]["id"], "acme");
    assert_eq!(body["clients"][0]["displayName"], "Acme");
    assert_eq!(body["clients"][0]["hosts"][0], "www.example.com");
}

#[tokio::test]
async fn a_client_detail_names_its_realm_but_not_its_roles() {
    // Roles belong to the identity view. Two copies is one that can be stale.
    let plane = control_plane();

    let body = json(
        send(
            &plane.router,
            as_operator("GET", "/api/clients/acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;

    assert_eq!(body["realm"], "acme");
    assert!(body.get("roles").is_none());
}

#[tokio::test]
async fn identity_is_returned_with_its_revision_and_reconciliation_state() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    let tag = entity_tag(&response);
    let body = json(response).await;

    assert_eq!(body["realm"], "acme");
    assert_eq!(body["roles"][0], "Client Realm Administrator");
    assert_eq!(body["clients"][0]["id"], "web");
    assert_eq!(body["revision"], tag, "the entity tag and the body must agree");

    // Nothing has reconciled it yet, and the API says so rather than implying
    // the document is reality.
    assert_eq!(body["reconciliation"]["status"], "pending");
}

#[tokio::test]
async fn a_write_at_the_current_revision_is_accepted_and_reports_pending() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let body = json(response).await;
    assert_eq!(body["roles"][2], "Invoicing Approver");
    assert_ne!(body["revision"], plane.revision.as_str());

    // The write succeeded; Keycloak has provably not been touched.
    assert_eq!(body["reconciliation"]["status"], "pending");
}

#[tokio::test]
async fn a_write_at_a_stale_revision_is_a_conflict() {
    let plane = control_plane();
    let stale = format!("\"{}\"", plane.revision);

    let first = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, &stale)
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);

    let second = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, &stale)
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    assert_eq!(second.status(), StatusCode::CONFLICT);
    assert_eq!(json(second).await["error"]["code"], "revision_conflict");
}

#[tokio::test]
async fn a_write_without_if_match_is_refused_rather_than_applied() {
    // Last-writer-wins is what this status exists to prevent.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
    assert_eq!(json(response).await["error"]["code"], "revision_required");
}

#[tokio::test]
async fn changing_the_realm_is_refused() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "somewhere-else",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "realm_immutable");
}

#[tokio::test]
async fn removing_a_required_role_is_refused() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "acme",
                    "roles": ["Client Realm User"],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn an_unknown_client_is_a_not_found() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/nobody/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(response).await["error"]["code"], "unknown_client");
}

#[tokio::test]
async fn an_edit_preserves_sections_the_control_plane_does_not_model() {
    let plane = control_plane();

    send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;

    let stored = plane
        .repository
        .get(&ClientId::try_new("acme").unwrap())
        .await
        .expect("the client must still be there");

    let document = stored.document.render().expect("the stored document must render");
    assert!(document.contains("invoicing: true"));
    assert!(document.contains("Invoicing Approver"));
}

#[tokio::test]
async fn an_unknown_field_in_the_body_is_refused_rather_than_ignored() {
    // Accepting it would report success for a change that did not happen.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "acme",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                    "keycloakRealmSettings": {"bruteForceProtected": true},
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn a_malformed_body_is_refused_in_this_api_s_error_shape() {
    // Axum's own rejection would answer in a different shape, and the console
    // branches on `error.code`.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{not json"))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json(response).await["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn a_client_id_that_could_not_exist_is_refused_in_this_api_s_error_shape() {
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("GET", "/api/clients/NOT_A_CLIENT/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");
    // The value is caller-controlled and reaches here from a URL; reflecting it
    // would turn the error body into a mirror.
    assert!(!body["error"]["message"].to_string().contains("NOT_A_CLIENT"));
}

#[tokio::test]
async fn an_unauthenticated_request_reaches_no_handler() {
    let plane = control_plane();

    for (method, path) in [
        ("GET", "/api/clients"),
        ("GET", "/api/clients/acme"),
        ("GET", "/api/clients/acme/identity"),
        ("PUT", "/api/clients/acme/identity"),
    ] {
        let response = send(
            &plane.router,
            http::Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} answered without an operator"
        );
    }
}

#[tokio::test]
async fn a_request_carrying_no_operator_identity_is_refused() {
    // This used to assert that a name outside the allowlist was refused, which
    // was a property of the trusted-header posture. That posture is gone: who
    // counts as an operator is now a realm role, checked against a verified
    // token by `OidcOperators`' own tests.
    //
    // What still belongs here is the property this *router* has to have —
    // every path under `/api/clients` refuses a request that established no
    // operator at all, which is the extractor doing its job.
    let plane = control_plane();

    let response = send(
        &plane.router,
        http::Request::builder()
            .method("GET")
            .uri("/api/clients")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json(response).await["error"]["code"], "unauthenticated");
}

/// A client body carrying one redirect strategy and one callback, otherwise
/// identical to [`identity_with_extra_role`]'s.
fn identity_with_one_client(id: &str, strategy: &str, uri: &str) -> Body {
    Body::from(
        serde_json::json!({
            "realm": "acme",
            "roles": ["Client Realm Administrator", "Client Realm User"],
            "clients": [{
                "id": id,
                "type": "oidc",
                "pkce": "s256",
                "redirect": {
                    "strategy": strategy,
                    "uris": [uri],
                },
            }],
        })
        .to_string(),
    )
}

#[tokio::test]
async fn a_plain_pkce_method_is_refused_at_the_api() {
    // ADR 0019 §3: `plain` is not a variant `PkceMethod` can hold, so the
    // refusal has to come from the model's own deserialiser reaching through
    // `BoundedJson` — not from a hand-written "if pkce == plain" check that
    // could drift from the type it is supposed to be guarding.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "acme",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                    "clients": [{
                        "id": "web",
                        "type": "oidc",
                        "pkce": "plain",
                        "redirect": {
                            "strategy": "claimedHttps",
                            "uris": ["https://www.example.com/callback"],
                        },
                    }],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");

    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("plain"), "{message}");
    assert!(message.contains("s256"), "{message}");
}

#[tokio::test]
async fn a_custom_scheme_strategy_is_refused_at_the_api_naming_the_deferral() {
    // ADR 0019 §3: `customScheme` is representable so a document does not
    // have to change shape again when the phase lands, and refused at
    // validation until it does. `client_rules::check_strategy_is_carried` is
    // what names the phase; this pins that its refusal reaches the operator
    // through the API rather than only through the model's own tests.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({
                    "realm": "acme",
                    "roles": ["Client Realm Administrator", "Client Realm User"],
                    "clients": [{
                        "id": "desktop",
                        "type": "oidc",
                        "pkce": "s256",
                        "redirect": {
                            "strategy": "customScheme",
                            "scheme": "nz.fieldstate.slipway",
                            "uris": ["nz.fieldstate.slipway:/callback"],
                        },
                    }],
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");

    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("Lane E phase 2"), "{message}");
}

#[tokio::test]
async fn a_client_carrying_a_callback_its_strategy_does_not_admit_is_refused_at_the_api() {
    // ADR 0019 §3: a `claimedHttps` strategy admits only public https
    // callbacks. `http://127.0.0.1/cb` is a loopback callback, so it belongs
    // under `development`, not here — and the refusal names both the strategy
    // and the URI's kind rather than silently reclassifying it.
    let plane = control_plane();

    let response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_one_client(
                "web",
                "claimedHttps",
                "http://127.0.0.1/cb",
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json(response).await;
    assert_eq!(body["error"]["code"], "invalid_request");

    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("claimedHttps"), "{message}");
    assert!(message.contains("loopback"), "{message}");
}

#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn a_native_client_is_declared_reconciled_and_read_back_with_its_pkce_and_strategy() {
    // docs/delivery.md's rule: one test through the surface an operator
    // actually uses, doing the thing this slice exists to let them do. This is
    // the test `docs/architecture/identity-edge-test-matrix.md` §F names.
    //
    // GET identity → read the revision
    // PUT identity declaring a v2 native client: pkce s256, strategy
    //     development, loopback callback, with If-Match
    // → 200, reconciliation pending
    // sweep against the fake provider
    // → the provider holds one public client with the S256 challenge method,
    //   the audience mapper, and exactly the declared callback
    // GET identity → the strategy and method come back as written
    // a second sweep changes nothing
    // PUT the same client with strategy claimedHttps and the loopback
    //     callback → 400 invalid_request, naming the strategy and the URI
    //
    // `too_many_lines` is allowed here for the same reason `fabric-fga-auth`'s
    // `whole_path.rs` allows it: this composed proof drives the real router
    // through a whole operator workflow, and splitting it into helpers named
    // "step one" and "step two" would hide the sequence rather than clarify it.
    //
    // What this test does *not* guard: `matches()` deciding "converged" for
    // the right reasons. `FakeIdentityProvider::write_client` echoes back
    // exactly what was declared — challenge method, audience mapper, enabled,
    // standard flow, and the post-logout term are all written unconditionally
    // — so every comparison below would still pass even if `matches()` never
    // looked at one of those fields at all. The actual drift guards are
    // `crates/fabric-reconciliation/src/plan/diff_tests.rs`: C5/C6
    // (`a_client_without_a_recognised_challenge_method_is_corrected`), C6a
    // (`a_client_whose_audience_mapper_was_removed_is_corrected`), D13
    // (`a_redirect_uri_this_model_cannot_parse_is_drift`), D13b
    // (`an_extra_redirect_uri_the_provider_holds_is_drift`), and A13b
    // (`a_client_carrying_a_mapper_nobody_declared_is_corrected`).
    let provider = Arc::new(FakeIdentityProvider::new());
    let plane = control_plane_with_identity_provider(provider.clone());

    let initial = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let revision = entity_tag(&initial);

    let put_response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_one_client(
                "native",
                "development",
                "http://127.0.0.1/callback",
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(put_response.status(), StatusCode::OK);
    assert_eq!(json(put_response).await["reconciliation"]["status"], "pending");

    // `put_identity` spawns its convergence with `converge::in_background`
    // rather than awaiting it (ADR 0008: the write is answered before the
    // identity provider is touched). On this test's current-thread runtime
    // that spawned task makes no progress until this future yields, so
    // without this it would run at some unpredictable later point — maybe
    // interleaved with the explicit sweep below, maybe not until after
    // `clear_calls()` has already been read, silently corrupting the exact
    // call-log assertion the second sweep depends on. Yielding once here
    // drains it deterministically instead of relying on the request path
    // below happening to be pending at the right moment.
    tokio::task::yield_now().await;

    let sweep = send(
        &plane.router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(sweep.status(), StatusCode::OK);
    assert_eq!(json(sweep).await["clients"], 1);

    let realm = provider
        .realm(&RealmName::try_new("acme").unwrap())
        .expect("the sweep must have created the realm");
    let observed = realm
        .clients
        .get(&OidcClientId::try_new("native").unwrap())
        .expect("the sweep must have written the declared client");

    // All nine fields `ObservedOidcClient` carries (`observed.rs:50-147`).
    // This proves the values round-trip through the API, the plan, and the
    // fake's own write — see this test's docstring for what it does not prove.
    assert!(observed.public, "every declared client is written as public");
    assert_eq!(
        observed.unmodellable_redirect_uris, 0,
        "a client this test declared has nothing the model cannot parse"
    );
    assert_eq!(
        observed.redirect_uris,
        BTreeSet::from([RedirectUri::try_new("http://127.0.0.1/callback").unwrap()]),
        "the observed set must be exactly the declared callback, no more and no less"
    );
    assert_eq!(observed.challenge_method, Some(PkceMethod::S256));
    assert_eq!(
        observed.audience_mapper.as_deref(),
        provider.configured_audience(),
        "the written mapper must assert the provider's own configured audience"
    );
    assert_eq!(
        observed.other_protocol_mappers, 0,
        "a client this test declared carries nothing beyond the one audience mapper"
    );
    assert!(observed.enabled, "a declared client is always written enabled");
    assert!(
        observed.standard_flow_enabled,
        "a declared client is written running the authorization-code flow"
    );
    assert!(
        observed.post_logout_redirect_uris_is_every_registered_uri,
        "the post-logout set must be written as every registered redirect URI"
    );

    let read_back = send(
        &plane.router,
        as_operator("GET", "/api/clients/acme/identity")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let read_tag = entity_tag(&read_back);
    let body = json(read_back).await;

    assert_eq!(body["clients"][0]["id"], "native");
    assert_eq!(body["clients"][0]["pkce"], "s256");
    assert_eq!(body["clients"][0]["redirect"]["strategy"], "development");
    assert_eq!(
        body["clients"][0]["redirect"]["uris"][0],
        "http://127.0.0.1/callback"
    );
    // The sweep converged the client, and the API says so rather than leaving
    // the console to infer it from the absence of an error.
    assert_eq!(body["reconciliation"]["status"], "applied");

    let stored = plane
        .repository
        .get(&ClientId::try_new("acme").unwrap())
        .await
        .expect("the client must still be there");
    assert_eq!(
        stored.document.api_version(),
        fabric_client_model::API_VERSION_V2,
        "an edit through this endpoint migrates the stored document in place (matrix row E14)"
    );

    provider.clear_calls();
    let second_sweep = send(
        &plane.router,
        as_operator("POST", "/api/reconciliation")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(second_sweep.status(), StatusCode::OK);
    assert_eq!(
        provider.calls(),
        vec!["observe_realm:acme"],
        "a second sweep against an unchanged declaration reads the realm and writes nothing"
    );

    let mismatched = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{read_tag}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_one_client(
                "native",
                "claimedHttps",
                "http://127.0.0.1/callback",
            ))
            .unwrap(),
    )
    .await;

    assert_eq!(mismatched.status(), StatusCode::BAD_REQUEST);
    let error = json(mismatched).await;
    assert_eq!(error["error"]["code"], "invalid_request");

    let message = error["error"]["message"].as_str().unwrap();
    assert!(message.contains("claimedHttps"), "{message}");
    assert!(message.contains("loopback"), "{message}");
}

/// ADR 0019: "the console shows the document's version, and says that an edit
/// will migrate it." This proves the wire half of that at the API the console
/// calls — the `acme` fixture (`ACME` in `tests/support/mod.rs`) is `v1`, and
/// an edit through this endpoint migrates it to `v2` in place (`with_identity`
/// re-parses the rendered document, so there is no path that writes a `v2`
/// client shape under a `v1` `apiVersion`).
#[tokio::test]
async fn identity_reports_the_document_s_schema_version_before_and_after_an_edit() {
    let plane = control_plane();

    let before = json(
        send(
            &plane.router,
            as_operator("GET", "/api/clients/acme/identity")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(before["apiVersion"], fabric_client_model::API_VERSION);

    let put_response = send(
        &plane.router,
        as_operator("PUT", "/api/clients/acme/identity")
            .header(header::IF_MATCH, format!("\"{}\"", plane.revision))
            .header(header::CONTENT_TYPE, "application/json")
            .body(identity_with_extra_role())
            .unwrap(),
    )
    .await;
    assert_eq!(put_response.status(), StatusCode::OK);
    assert_eq!(
        json(put_response).await["apiVersion"],
        fabric_client_model::API_VERSION_V2
    );

    let after = json(
        send(
            &plane.router,
            as_operator("GET", "/api/clients/acme/identity")
                .body(Body::empty())
                .unwrap(),
        )
        .await,
    )
    .await;
    assert_eq!(after["apiVersion"], fabric_client_model::API_VERSION_V2);
}
