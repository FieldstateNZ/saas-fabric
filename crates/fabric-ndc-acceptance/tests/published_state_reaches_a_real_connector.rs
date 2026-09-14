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
use fabric_connector_ndc::{
    build_ndc_connector, CollectionProcedures, NdcConnectorConfig, PayloadShape, ProcedureBinding,
};
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

/// The write-enabled twin of [`read_only_config`]: maps `articles`' insert,
/// update and delete to the real procedures the connector's own schema
/// declares
/// (`crates/fabric-ndc-acceptance/tests/fixtures/ndc-postgres-v3.1.0/README.md`).
///
/// `insert_articles`' `objects` carries the payload; there is no
/// `filter_argument` because `MutationSpec::Insert` never carries a predicate
/// -- the tenant discriminator is stamped onto the row instead
/// (`fabric_connector::MutationSpec::for_target`).
///
/// The real `update_articles_by_id_and_tenant_key` and
/// `delete_articles_by_id_and_tenant_key` procedures are keyed by primary
/// key, not by a bare predicate: both require `key_id` and `key_tenant_key`
/// alongside their optional `pre_check`. `key_arguments` names which of the
/// predicate's own equalities are repeated as those two named arguments --
/// this is issue #67's F3 fix, and is what this file could not express
/// before it (see the now-deleted comment this rustdoc replaces). The
/// tenant discriminator therefore reaches the connector twice on a keyed
/// write: once as `key_tenant_key`, once inside `pre_check` -- defence in
/// depth, not redundancy (`ProcedureBinding::key_arguments`'s own rustdoc).
/// `update_columns` takes `PayloadShape::SetOperations`
/// (`{col: {"_set": value}}`), which is what `ndc-postgres` generates for a
/// keyed update; `insert_articles`' `objects` stays `PayloadShape::Values`.
fn writable_config(endpoint: String) -> NdcConnectorConfig {
    let key_arguments = BTreeMap::from([
        ("id".to_owned(), "key_id".to_owned()),
        ("tenant_key".to_owned(), "key_tenant_key".to_owned()),
    ]);

    let mut procedures = BTreeMap::new();
    procedures.insert(
        "articles".to_owned(),
        CollectionProcedures {
            insert: Some(ProcedureBinding {
                procedure: "insert_articles".to_owned(),
                payload_argument: Some("objects".to_owned()),
                filter_argument: None,
                key_arguments: BTreeMap::new(),
                payload_shape: PayloadShape::Values,
            }),
            update: Some(ProcedureBinding {
                procedure: "update_articles_by_id_and_tenant_key".to_owned(),
                payload_argument: Some("update_columns".to_owned()),
                filter_argument: Some("pre_check".to_owned()),
                key_arguments: key_arguments.clone(),
                payload_shape: PayloadShape::SetOperations,
            }),
            delete: Some(ProcedureBinding {
                procedure: "delete_articles_by_id_and_tenant_key".to_owned(),
                payload_argument: None,
                filter_argument: Some("pre_check".to_owned()),
                key_arguments,
                payload_shape: PayloadShape::Values,
            }),
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

#[tokio::test]
async fn a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives() {
    let test_name = "a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(writable_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the connector's schema should accept the keyed delete mapping");
    let composed = compose(connector, &fixtures::writable_snapshot()).await;

    // acme creates the row this test's cross-tenant delete will target, via
    // `POST` -- the seed SQL stays untouched, so the pre-existing
    // `count(*) FROM articles` assertions elsewhere in this file keep
    // holding.
    let create = composed
        .app
        .clone()
        .oneshot(requests::post(
            "/articles",
            &requests::claims_for("acme"),
            &serde_json::json!({"id": "2", "title": "Acme Second"}),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);

    // globex asks to delete acme's row by its logical key. The keyed
    // procedure requires `key_tenant_key`, which `key_arguments` fills from
    // the predicate `for_target` built for globex -- so the delete reaches
    // the connector scoped to globex's own discriminator value, not acme's.
    let response = composed
        .app
        .clone()
        .oneshot(requests::delete("/articles/2", &requests::claims_for("globex")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = requests::body_json(response).await;
    assert_eq!(body["affected"], 0, "{body}");

    // Read directly against postgres: the row survives, untouched.
    let count = stack
        .query_scalar("SELECT count(*) FROM articles WHERE id = '2' AND tenant_key = 'tenant-acme-482';");
    assert_eq!(count, "1");

    // acme still owns it; globex still cannot see it.
    let acme_read = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/2", &requests::claims_for("acme")))
        .await
        .unwrap();
    assert_eq!(acme_read.status(), StatusCode::OK);

    let globex_read = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/2", &requests::claims_for("globex")))
        .await
        .unwrap();
    assert_eq!(globex_read.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_keyed_delete_removes_only_this_tenants_row_under_the_shared_key() {
    let test_name = "a_keyed_delete_removes_only_this_tenants_row_under_the_shared_key";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(writable_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the connector's schema should accept the keyed delete mapping");
    let composed = compose(connector, &fixtures::writable_snapshot()).await;

    // acme deletes its own row under the logical key `1` -- the same key
    // globex's row (seeded by `SEED_SQL`) also shares.
    let response = composed
        .app
        .clone()
        .oneshot(requests::delete("/articles/1", &requests::claims_for("acme")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = requests::body_json(response).await;
    assert_eq!(body["affected"], 1, "{body}");

    // Read directly against postgres: one physical row remains under id
    // `1`, and it is globex's.
    let count = stack.query_scalar("SELECT count(*) FROM articles WHERE id = '1';");
    assert_eq!(count, "1");
    let surviving_tenant = stack.query_scalar("SELECT tenant_key FROM articles WHERE id = '1';");
    assert_eq!(surviving_tenant, fixtures::GLOBEX_DISCRIMINATOR_VALUE);

    // globex's own read is unaffected by acme's delete.
    let globex_read = composed
        .app
        .clone()
        .oneshot(requests::get("/articles/1", &requests::claims_for("globex")))
        .await
        .unwrap();
    assert_eq!(globex_read.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_keyed_update_changes_only_this_tenants_row() {
    let test_name = "a_keyed_update_changes_only_this_tenants_row";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(writable_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the connector's schema should accept the keyed update mapping");
    let composed = compose(connector, &fixtures::writable_snapshot()).await;

    let response = composed
        .app
        .clone()
        .oneshot(requests::patch(
            "/articles/1",
            &requests::claims_for("acme"),
            &serde_json::json!({"title": "Acme Handbook, revised"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = requests::body_json(response).await;
    assert_eq!(body["affected"], 1, "{body}");

    // Read directly against postgres: only acme's physical row changed.
    let acme_title =
        stack.query_scalar("SELECT title FROM articles WHERE id = '1' AND tenant_key = 'tenant-acme-482';");
    assert_eq!(acme_title, "Acme Handbook, revised");
    let globex_title =
        stack.query_scalar("SELECT title FROM articles WHERE id = '1' AND tenant_key = 'tenant-globex-915';");
    assert_eq!(globex_title, "Globex Playbook");

    // globex's own read agrees -- unaffected by acme's update.
    let globex_read = requests::body_json(
        composed
            .app
            .clone()
            .oneshot(requests::get("/articles/1", &requests::claims_for("globex")))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(globex_read["title"], "Globex Playbook", "{globex_read}");
}

#[tokio::test]
async fn no_write_response_names_the_key_arguments_or_the_procedure() {
    let test_name = "no_write_response_names_the_key_arguments_or_the_procedure";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let stack = Stack::up(ConnectorMode::Static);
    let connector = build_ndc_connector(writable_config(stack.connector_base_url.clone()), None)
        .await
        .expect("the connector's schema should accept the keyed write mappings");
    let composed = compose(connector, &fixtures::writable_snapshot()).await;

    // Mirrors the sequence of writes tests 1-3 perform, all within this
    // test's own stack, so their response bodies can be inspected here --
    // the cross-tenant delete first (it removes nothing), then the update
    // (the row must still exist), and the keyed delete last (it removes the
    // row, so nothing after it needs the row intact).
    let create = composed
        .app
        .clone()
        .oneshot(requests::post(
            "/articles",
            &requests::claims_for("acme"),
            &serde_json::json!({"id": "2", "title": "Acme Second"}),
        ))
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);

    let cross_tenant_delete = requests::body_text(
        composed
            .app
            .clone()
            .oneshot(requests::delete("/articles/2", &requests::claims_for("globex")))
            .await
            .unwrap(),
    )
    .await;

    let update = requests::body_text(
        composed
            .app
            .clone()
            .oneshot(requests::patch(
                "/articles/1",
                &requests::claims_for("acme"),
                &serde_json::json!({"title": "Acme Handbook, revised"}),
            ))
            .await
            .unwrap(),
    )
    .await;

    let keyed_delete = requests::body_text(
        composed
            .app
            .clone()
            .oneshot(requests::delete("/articles/1", &requests::claims_for("acme")))
            .await
            .unwrap(),
    )
    .await;

    // Everything the wire format could leak but a `WriteResponse` must not:
    // the procedures' own names, their key and predicate argument names, the
    // `_set` wrapping `payload_shape` adds, and either tenant's
    // discriminator value.
    for text in [&cross_tenant_delete, &update, &keyed_delete] {
        for forbidden in [
            "key_id",
            "key_tenant_key",
            "pre_check",
            "update_columns",
            "_set",
            "by_id_and_tenant_key",
            fixtures::ACME_DISCRIMINATOR_VALUE,
            fixtures::GLOBEX_DISCRIMINATOR_VALUE,
        ] {
            assert!(!text.contains(forbidden), "{forbidden} leaked in {text}");
        }
    }
}
