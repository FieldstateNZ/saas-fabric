//! The composed acceptance test issue #62 exists for: publish a fixture
//! through the real `FilesystemRuntimePublication`, negotiate the real
//! `fabric-connector-ndc` adapter against a running `ghcr.io/hasura/ndc-postgres`
//! process, and drive the real `fabric_tenant_runtime::build_runtime` and
//! `fabric_data_api::build_data_api` over both -- with two tenants sharing
//! one physical table under discriminator isolation.
//!
//! `docs/delivery.md`'s rule is what this file exists to satisfy one layer
//! down from where `fabric-runtime-publication`'s own composed test already
//! satisfies it: that test proves the publisher and the Data API agree on
//! the wire, against a recording connector that only ever applied whatever
//! predicate it was given to a corpus it already held in memory. Nothing
//! before this file has proven the predicate the platform builds is a
//! predicate a real database applies -- that a connector cannot ignore the
//! routing argument, drop the tenant conjunct, or otherwise silently serve
//! more than it was asked for. §2 of the plan calls this out directly:
//! round six of review found two defects invisible to every unit test
//! because the requests were well-formed and the logic correct, and both
//! were only found once bytes from a real connector answered.
//!
//! The corpus is real SQL, seeded by literal in `support::postgres::SEED_SQL`
//! -- never a Rust constant the fixture could move alongside a mutation
//! (`docs/verification.md` row 1a's lesson, applied here with a database
//! instead of a fake).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use fabric_connector::ConnectorId;
use fabric_connector_ndc::{build_ndc_connector, CollectionProcedures, NdcConnectorConfig, ProcedureBinding};
use http::StatusCode;
use serde_json::Value;
use support::compose::compose;
use support::connector::ConnectorMode;
use support::gate::docker_available_or_skip;
use support::impostor::Impostor;
use support::stack::Stack;
use support::{fixtures, requests};
use tower::ServiceExt as _;

/// A read-only connector configuration: no routing argument, no writes.
/// What every isolation and fail-closed test in this file negotiates
/// against the static-mode connector with.
fn read_only_config(endpoint: String) -> NdcConnectorConfig {
    NdcConnectorConfig {
        id: ConnectorId::try_new(fixtures::CONNECTOR_ID).unwrap(),
        endpoint,
        http_timeout_seconds: 10,
        http_connect_timeout_seconds: 5,
        connection_name_argument: None,
        connection_string_argument: None,
        procedures: BTreeMap::new(),
    }
}

/// The write-enabled twin of [`read_only_config`]: maps `articles`' insert to
/// the real `insert_articles` procedure the connector's own schema declares
/// (`crates/fabric-ndc-acceptance/tests/fixtures/ndc-postgres-v3.1.0/README.md`).
/// `objects` carries the payload; there is no `filter_argument` because
/// `MutationSpec::Insert` never carries a predicate -- the tenant
/// discriminator is stamped onto the row instead
/// (`fabric_connector::MutationSpec::for_target`). Update and delete are
/// deliberately absent: the real `update_articles_by_id_and_tenant_key` and
/// `delete_articles_by_id_and_tenant_key` procedures require `key_id` and
/// `key_tenant_key` arguments `CollectionProcedures` has nowhere to carry --
/// F3 in the issue #62 plan, deferred to its own follow-up issue rather than
/// grown here (ADR 0004's addendum; `docs/verification.md`).
fn writable_config(endpoint: String) -> NdcConnectorConfig {
    let mut procedures = BTreeMap::new();
    procedures.insert(
        "articles".to_owned(),
        CollectionProcedures {
            insert: Some(ProcedureBinding {
                procedure: "insert_articles".to_owned(),
                payload_argument: Some("objects".to_owned()),
                filter_argument: None,
            }),
            update: None,
            delete: None,
        },
    );

    NdcConnectorConfig {
        id: ConnectorId::try_new(fixtures::CONNECTOR_ID).unwrap(),
        endpoint,
        http_timeout_seconds: 10,
        http_connect_timeout_seconds: 5,
        connection_name_argument: None,
        connection_string_argument: None,
        procedures,
    }
}

#[tokio::test]
async fn two_tenants_sharing_one_physical_table_each_receive_only_their_own_row() {
    let test_name = "two_tenants_sharing_one_physical_table_each_receive_only_their_own_row";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    for (tenant, title) in [("acme", "Acme Handbook"), ("globex", "Globex Playbook")] {
        let response = composed
            .app
            .clone()
            .oneshot(requests::get("/articles", &requests::claims_for(tenant)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tenant}");

        let body = requests::body_json(response).await;
        let rows = body["data"]
            .as_array()
            .unwrap_or_else(|| panic!("{tenant}: {body}"));
        assert_eq!(rows.len(), 1, "{tenant} should see only its own row: {body}");
        assert_eq!(rows[0]["title"], title, "{tenant}");
    }
}

#[tokio::test]
async fn the_same_logical_article_key_reaches_a_different_physical_row_for_each_tenant() {
    let test_name = "the_same_logical_article_key_reaches_a_different_physical_row_for_each_tenant";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    let acme = requests::body_json(
        composed
            .app
            .clone()
            .oneshot(requests::get("/articles/1", &requests::claims_for("acme")))
            .await
            .unwrap(),
    )
    .await;
    let globex = requests::body_json(
        composed
            .app
            .clone()
            .oneshot(requests::get("/articles/1", &requests::claims_for("globex")))
            .await
            .unwrap(),
    )
    .await;

    // Same logical key (`/articles/1`), two different physical rows -- the
    // whole point of the shared-table fixture.
    assert_eq!(acme["title"], "Acme Handbook", "{acme}");
    assert_eq!(globex["title"], "Globex Playbook", "{globex}");
    assert_ne!(acme, globex);
}

#[tokio::test]
async fn both_tenants_rows_really_are_in_the_one_table_the_query_narrowed() {
    let test_name = "both_tenants_rows_really_are_in_the_one_table_the_query_narrowed";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    // Read directly against postgres, bypassing the connector under test --
    // "the row really exists" cannot be proven by the same connector whose
    // predicate is what is being tested.
    let count = stack.query_scalar("SELECT count(*) FROM articles;");
    assert_eq!(
        count, "2",
        "both tenants' rows should physically be in the one shared table"
    );

    for (tenant, title) in [("acme", "Acme Handbook"), ("globex", "Globex Playbook")] {
        let body = requests::body_json(
            composed
                .app
                .clone()
                .oneshot(requests::get("/articles", &requests::claims_for(tenant)))
                .await
                .unwrap(),
        )
        .await;
        let rows = body["data"].as_array().unwrap();
        assert_eq!(
            rows.len(),
            1,
            "{tenant}: the query narrowed the table's two physical rows to this tenant's one"
        );
        assert_eq!(rows[0]["title"], title, "{tenant}");
    }
}

#[tokio::test]
async fn neither_request_names_a_tenant_and_a_tenant_header_is_refused_outright() {
    let test_name = "neither_request_names_a_tenant_and_a_tenant_header_is_refused_outright";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    let request = requests::get_with_tenant_header("/articles", &requests::claims_for("acme"), "globex");
    let response = composed.app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // What this does NOT assert, and why: this suite has no request counter
    // on the real connector the way `fabric-runtime-publication`'s
    // recording connector carries one, so there is nothing here to read
    // "the connector saw zero queries" off directly. What IS true, and is a
    // property of the code path rather than of this one observation:
    // `fabric_identity::resolver::IdentityResolver::resolve` rejects a
    // present `x-tenant-id` header (step 1 of its own rustdoc) inside the
    // `TenantIdentity` axum extractor, which runs to completion or fails
    // before the handler body -- and therefore before DataSource resolution
    // or any connector call -- is ever reached. The 400 above is that
    // rejection; nothing between it and this assertion could have reached
    // the connector.
}

#[tokio::test]
async fn the_predicate_the_platform_built_is_the_predicate_the_database_applied() {
    let test_name = "the_predicate_the_platform_built_is_the_predicate_the_database_applied";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    // What this test asserts, precisely: the physical table holds both
    // tenants' rows -- proved directly against postgres, bypassing the
    // connector under test -- and each tenant's request through the
    // composed router returns exactly its own single row. Those two facts
    // are only consistent with a query that reached the real connector
    // carrying a predicate that narrowed two rows to one; an unpredicated
    // query against this table returns both, which
    // `both_tenants_rows_really_are_in_the_one_table` in
    // `the_stack_comes_up.rs` establishes directly, and which the mutation
    // experiment in `docs/verification.md` confirms by disabling the
    // predicate and watching this same pair of facts go inconsistent (both
    // tenants receive both rows).
    //
    // What this test does NOT assert: the literal NDC expression or SQL the
    // connector executed. `NdcHttpClient` is private to
    // `fabric-connector-ndc` (ADR 0001 keeps NDC vocabulary inside that
    // crate), so there is no wire request to inspect from here -- a raw
    // query through the connector's own client, as the plan considered and
    // rejected, is not possible without breaking that containment.
    let count = stack.query_scalar("SELECT count(*) FROM articles;");
    assert_eq!(count, "2");

    for (tenant, title) in [("acme", "Acme Handbook"), ("globex", "Globex Playbook")] {
        let body = requests::body_json(
            composed
                .app
                .clone()
                .oneshot(requests::get("/articles", &requests::claims_for(tenant)))
                .await
                .unwrap(),
        )
        .await;
        let rows = body["data"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{tenant}");
        assert_eq!(rows[0]["title"], title, "{tenant}");
    }
}

#[tokio::test]
async fn no_response_names_the_table_the_connector_or_the_discriminator() {
    let test_name = "no_response_names_the_table_the_connector_or_the_discriminator";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    let list = requests::body_text(
        composed
            .app
            .clone()
            .oneshot(requests::get("/articles", &requests::claims_for("globex")))
            .await
            .unwrap(),
    )
    .await;
    let single = requests::body_text(
        composed
            .app
            .clone()
            .oneshot(requests::get("/articles/1", &requests::claims_for("globex")))
            .await
            .unwrap(),
    )
    .await;

    // The physical collection name coincides with the public resource name
    // ("articles" on both sides, per the fixture) so its absence is not
    // asserted -- a caller already knows it from the route it called. What
    // is checked is everything the wire format could leak but the response
    // shape does not: the connector id, the discriminator column, and both
    // tenants' discriminator values.
    for text in [&list, &single] {
        assert!(!text.contains(fixtures::CONNECTOR_ID), "{text}");
        assert!(!text.contains(fixtures::DISCRIMINATOR_COLUMN), "{text}");
        assert!(!text.contains(fixtures::ACME_DISCRIMINATOR_VALUE), "{text}");
        assert!(!text.contains(fixtures::GLOBEX_DISCRIMINATOR_VALUE), "{text}");
    }

    let list_json: Value = serde_json::from_str(&list).unwrap();
    let row_keys: Vec<&String> = list_json["data"][0].as_object().unwrap().keys().collect();
    assert_eq!(row_keys, vec!["id", "title"]);

    let single_json: Value = serde_json::from_str(&single).unwrap();
    let single_keys: Vec<&String> = single_json.as_object().unwrap().keys().collect();
    assert_eq!(single_keys, vec!["id", "title"]);
}

#[tokio::test]
async fn a_connector_that_declares_no_routing_argument_is_refused_before_it_serves_anyone() {
    let test_name = "a_connector_that_declares_no_routing_argument_is_refused_before_it_serves_anyone";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);

    let mut config = read_only_config(stack.connector_base_url.clone());
    config.connection_name_argument = Some("connection_name".to_owned());

    let error = build_ndc_connector(config, None)
        .await
        .map(|_connector| ())
        .expect_err("the static connector's real schema declares no request-level arguments");

    assert!(
        error.contains("connection_name_argument")
            && error.contains("declares no request-level arguments at all"),
        "expected the no-routing-argument refusal naming the setting, got: {error}"
    );
}

#[tokio::test]
async fn a_connector_that_answers_http_but_not_ndc_is_refused_rather_than_believed() {
    let test_name = "a_connector_that_answers_http_but_not_ndc_is_refused_rather_than_believed";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let impostor = Impostor::start();

    let error = build_ndc_connector(read_only_config(impostor.base_url.clone()), None)
        .await
        .map(|_connector| ())
        .expect_err("nginx answers 200 with HTML, which is not an NDC capabilities document");

    assert!(
        error.contains("malformed response"),
        "expected the malformed-response refusal, got: {error}"
    );
}

#[tokio::test]
async fn a_stopped_connector_answers_service_unavailable_and_never_another_tenants_row() {
    let test_name = "a_stopped_connector_answers_service_unavailable_and_never_another_tenants_row";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let mut stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the static connector should negotiate with no routing configured");
    let composed = compose(connector, &fixtures::read_only_snapshot()).await;

    // A successful read first, so the 503 below is provably about the
    // connector going away mid-run rather than never having worked.
    let ok = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/1", &requests::claims_for("acme")))
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);

    stack.stop_connector();

    let response = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/1", &requests::claims_for("globex")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok()),
        Some("5")
    );

    let body = requests::body_json(response).await;
    assert_eq!(body["error"]["code"], "connector_unavailable", "{body}");
    // No row, from either tenant, ever appears in a failure body.
    let text = body.to_string();
    assert!(!text.contains("Globex Playbook"), "{text}");
    assert!(!text.contains("Acme Handbook"), "{text}");
}

#[tokio::test]
async fn the_version_this_client_advertises_is_the_one_the_connector_accepts() {
    let test_name = "the_version_this_client_advertises_is_the_one_the_connector_accepts";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);

    // The handshake succeeding at all is the proof: `check_version` inside
    // `build_ndc_connector` requires the connector's own `/capabilities` to
    // report at least the 0.2.4 floor this client advertises in
    // `X-Hasura-NDC-Version`, and refuses the connector otherwise (ADR
    // 0001). Neither the floor constant nor the negotiated version string is
    // exposed outside `fabric-connector-ndc` -- ADR 0001 keeps NDC
    // vocabulary, version numbers included, inside that one crate, the same
    // boundary §26 draws for applications above the Data API -- so there is
    // nothing more specific this crate can read back once negotiation has
    // folded the version into a pass/fail. The version floor's own
    // enforcement (a connector reporting below it is refused; the exact
    // header this client sends) is pinned directly against real captured
    // documents by `fabric-connector-ndc`'s own fixture-backed unit tests
    // (`registration::version_tests`,
    // `tests/fixtures/ndc-postgres-v3.1.0/capabilities.json`), which is
    // where that half of this acceptance criterion is actually proven.
    build_ndc_connector(read_only_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the real connector's advertised version should satisfy this client's floor");
}

#[tokio::test]
async fn a_write_the_connector_accepts_reports_the_count_the_connector_gave() {
    let test_name = "a_write_the_connector_accepts_reports_the_count_the_connector_gave";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(writable_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the connector's schema should accept the insert_articles mapping");
    let composed = compose(connector, &fixtures::writable_snapshot()).await;

    let response = composed
        .app
        .clone()
        .oneshot(requests::post(
            "/articles",
            &requests::claims_for("acme"),
            &serde_json::json!({"id": "3", "title": "Acme Appendix"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = requests::body_json(response).await;
    assert_eq!(body["affected"], 1, "{body}");

    // The platform's stamp, not the caller's payload, decided which
    // physical row this became: acme can read it back, globex cannot.
    let acme_read = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/3", &requests::claims_for("acme")))
        .await
        .unwrap();
    assert_eq!(acme_read.status(), StatusCode::OK);

    let globex_read = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/3", &requests::claims_for("globex")))
        .await
        .unwrap();
    assert_eq!(globex_read.status(), StatusCode::NOT_FOUND);

    // Read directly against postgres: exactly one row landed, not zero and
    // not two.
    let count = stack.query_scalar("SELECT count(*) FROM articles WHERE id = '3';");
    assert_eq!(count, "1");
}

// `a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives`
// is not implemented here. The real `delete_articles_by_id_and_tenant_key`
// procedure requires `key_id` and `key_tenant_key` arguments alongside its
// `pre_check` predicate, and `fabric_connector_ndc::CollectionProcedures`
// has nowhere to carry a required key argument -- a neutral
// `MutationSpec::Delete { filter }` cannot be expressed against this
// connector's generated procedures as they stand. This is F3 in the issue
// #62 plan; ADR 0004's addendum and the lead's decision on this issue both
// defer it to a new, separate issue ("neutral update/delete cannot be
// expressed against `ndc-postgres` v3.1.0's keyed procedures"), which will
// supersede ADR 0004 rather than amend it further. `docs/verification.md`
// records the same deferral in its falsified-assumptions table.
