//! The composed acceptance test ADR 0018 exists for: publish through the
//! real port, then drive the real consumer stack -- `build_runtime` over the
//! real `JsonFileSource`, and `build_data_api` over the real
//! `ResourceCatalog` -- with two tenants sharing one DataSource under
//! discriminator isolation.
//!
//! `docs/delivery.md` is the standard this file is held to: a vertical slice
//! is not complete until its primary workflow is exercised through the real
//! surface, and a test that cannot fail is worse than no test. Every
//! isolation assertion here is checked against the recording connector's
//! *captured predicate*, not only the response body, and the connector
//! applies that predicate to a shared corpus rather than dispatching on
//! tenant identity -- see `tests/support/connector.rs` for why that
//! distinction is load-bearing.

// Clippy's `allow-unwrap-in-tests` only covers `#[test]` functions
// themselves, not the fixture helpers every test here calls into -- an
// integration test file states it once here, as every other one in this
// workspace does.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use fabric_connector::{ComparisonOperator, Filter};
use fabric_runtime_publication::{
    DocumentInput, DocumentOutcome, DocumentRevision, PublicationError, RuntimePublication as _,
};
use http::StatusCode;
use serde_json::json;
use tower::ServiceExt as _;

/// The predicate a discriminator binding for `value` must produce.
fn tenant_predicate(value: &str) -> Filter {
    Filter::Compare {
        field: support::field("tenant_key"),
        operator: ComparisonOperator::Equal,
        value: json!(value),
    }
}

/// Tenants and data sources are read back through the real
/// `fabric_tenant_runtime::JsonFileSource`, over files `support::build_stack`
/// published through the real `FilesystemRuntimePublication` -- no test-only
/// parser stands in for either. The catalogue leg cannot reuse `fabric-api`'s
/// own `startup::catalog::load` (`pub(super)`, per ADR 0018's Consequences),
/// so `build_stack` reads `catalog.json` back and deserialises it as the
/// real, public `fabric_data_api::ResourceCatalog` -- the same type `load`
/// builds -- before handing it to the real `build_data_api`. The compiler
/// pins the tenants/data-sources half through `JsonFileSource`'s generic
/// parameter; this test's use of `ResourceCatalog` pins the other half.
#[tokio::test]
async fn the_runtime_serves_from_files_the_publisher_wrote() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    for (tenant, title) in [("acme", "Acme Handbook"), ("globex", "Globex Playbook")] {
        let response = stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles/1",
                &support::claims_for(tenant),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK, "{tenant}");
        let body = support::body_json(response).await;
        assert_eq!(body["title"], title, "{tenant}");
    }
}

#[tokio::test]
async fn two_tenants_sharing_one_data_source_each_receive_only_their_own_row() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let acme = support::body_json(
        stack
            .app
            .clone()
            .oneshot(support::request("GET", "/articles", &support::claims_for("acme")))
            .await
            .unwrap(),
    )
    .await;
    let globex = support::body_json(
        stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles",
                &support::claims_for("globex"),
            ))
            .await
            .unwrap(),
    )
    .await;

    let acme_rows = acme["data"].as_array().unwrap();
    let globex_rows = globex["data"].as_array().unwrap();

    assert_eq!(acme_rows.len(), 1, "{acme}");
    assert_eq!(globex_rows.len(), 1, "{globex}");
    assert_eq!(acme_rows[0]["title"], "Acme Handbook");
    assert_eq!(globex_rows[0]["title"], "Globex Playbook");
    assert_ne!(acme_rows[0], globex_rows[0]);
}

#[tokio::test]
async fn each_call_reaches_the_connector_carrying_only_its_own_tenant_predicate() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    stack
        .app
        .clone()
        .oneshot(support::request("GET", "/articles", &support::claims_for("acme")))
        .await
        .unwrap();
    let (_, acme_spec) = stack.connector.last_query();

    stack
        .app
        .clone()
        .oneshot(support::request(
            "GET",
            "/articles",
            &support::claims_for("globex"),
        ))
        .await
        .unwrap();
    let (_, globex_spec) = stack.connector.last_query();

    assert_eq!(
        acme_spec.filter,
        Some(tenant_predicate(support::ACME_DISCRIMINATOR_VALUE))
    );
    assert_eq!(
        globex_spec.filter,
        Some(tenant_predicate(support::GLOBEX_DISCRIMINATOR_VALUE))
    );
    assert_ne!(acme_spec.filter, globex_spec.filter);
}

#[tokio::test]
async fn the_same_logical_article_key_reaches_a_different_predicate_for_each_tenant() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    stack
        .app
        .clone()
        .oneshot(support::request(
            "GET",
            "/articles/1",
            &support::claims_for("acme"),
        ))
        .await
        .unwrap();
    let (_, acme_spec) = stack.connector.last_query();

    stack
        .app
        .clone()
        .oneshot(support::request(
            "GET",
            "/articles/1",
            &support::claims_for("globex"),
        ))
        .await
        .unwrap();
    let (_, globex_spec) = stack.connector.last_query();

    let Some(Filter::And {
        clauses: acme_clauses,
    }) = acme_spec.filter
    else {
        panic!(
            "a keyed read must carry both the key and the tenant predicate: {:?}",
            acme_spec.filter
        );
    };
    let Some(Filter::And {
        clauses: globex_clauses,
    }) = globex_spec.filter
    else {
        panic!(
            "a keyed read must carry both the key and the tenant predicate: {:?}",
            globex_spec.filter
        );
    };

    assert!(acme_clauses.contains(&tenant_predicate(support::ACME_DISCRIMINATOR_VALUE)));
    assert!(globex_clauses.contains(&tenant_predicate(support::GLOBEX_DISCRIMINATOR_VALUE)));
    assert_ne!(acme_clauses, globex_clauses);
}

#[tokio::test]
async fn no_response_names_the_data_source_the_two_tenants_share() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let list = support::body_json(
        stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles",
                &support::claims_for("globex"),
            ))
            .await
            .unwrap(),
    )
    .await;
    let single = support::body_json(
        stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles/1",
                &support::claims_for("globex"),
            ))
            .await
            .unwrap(),
    )
    .await;

    for body in [&list, &single] {
        let text = body.to_string();
        assert!(!text.contains(support::DATA_SOURCE_ID), "{text}");
        assert!(!text.contains(support::DISCRIMINATOR_COLUMN), "{text}");
    }

    let row_keys: Vec<&String> = list["data"][0].as_object().unwrap().keys().collect();
    assert_eq!(row_keys, vec!["id", "title"]);

    let single_keys: Vec<&String> = single.as_object().unwrap().keys().collect();
    assert_eq!(single_keys, vec!["id", "title"]);
}

#[tokio::test]
async fn a_caller_supplied_tenant_header_is_refused_against_published_state() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let request = support::request_with_tenant_header("/articles", &support::claims_for("acme"), "globex");

    let response = stack.app.clone().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(stack.connector.query_count(), 0);
}

#[tokio::test]
async fn a_stale_revision_publication_is_refused_and_the_last_good_files_remain() {
    let stack = support::build_stack(&support::base_snapshot(2)).await;

    let tenants_before = support::file_identity(&stack.dir.tenants_path());
    let data_sources_before = support::file_identity(&stack.dir.data_sources_path());

    let error = stack
        .dir
        .publisher()
        .publish(&support::base_snapshot(1))
        .await
        .unwrap_err();

    assert!(matches!(error, PublicationError::StaleRevision { .. }), "{error}");
    assert_eq!(support::file_identity(&stack.dir.tenants_path()), tenants_before);
    assert_eq!(
        support::file_identity(&stack.dir.data_sources_path()),
        data_sources_before
    );

    // The running stack was never touched, so it keeps serving both tenants
    // exactly as it did before the refused publication was attempted.
    for tenant in ["acme", "globex"] {
        let response = stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles/1",
                &support::claims_for(tenant),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tenant}");
    }
}

#[tokio::test]
async fn a_same_revision_publication_with_a_different_payload_is_refused() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    // Same revision as what is held, but acme's discriminator value has
    // changed -- different bytes at an unchanged revision.
    let mut divergent = support::base_snapshot(1);
    divergent.tenants.payload[0] = support::tenant_binding("acme", "tenant-acme-999", 1);

    let error = stack.dir.publisher().publish(&divergent).await.unwrap_err();

    assert!(
        matches!(error, PublicationError::DivergentPayload { .. }),
        "{error}"
    );
}

#[tokio::test]
async fn a_republication_at_the_same_revision_with_the_same_payload_writes_nothing() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let names = [
        "tenants.json",
        "tenants.manifest.json",
        "data-sources.json",
        "data-sources.manifest.json",
        "catalog.json",
        "catalog.manifest.json",
    ];
    let before: Vec<(u64, i64)> = names
        .iter()
        .map(|name| support::file_identity(&stack.dir.path().join(name)))
        .collect();

    let report = stack
        .dir
        .publisher()
        .publish(&support::base_snapshot(1))
        .await
        .unwrap();

    assert_eq!(report.tenants, DocumentOutcome::Unchanged);
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Unchanged);

    let after: Vec<(u64, i64)> = names
        .iter()
        .map(|name| support::file_identity(&stack.dir.path().join(name)))
        .collect();
    assert_eq!(before, after);
}

#[tokio::test]
async fn a_publication_naming_a_data_source_it_does_not_publish_is_refused_before_any_write() {
    let dir = support::TempDir::new();

    let mut snapshot = support::base_snapshot(1);
    snapshot.data_sources.payload.clear();

    let error = dir.publisher().publish(&snapshot).await.unwrap_err();

    assert!(
        matches!(error, PublicationError::DanglingDataSource { .. }),
        "{error}"
    );
    assert!(!dir.tenants_path().exists());
    assert!(!dir.data_sources_path().exists());
    assert!(!dir.catalog_path().exists());
}

#[tokio::test]
async fn a_failed_refresh_leaves_the_runtime_serving_the_last_good_snapshot() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    for tenant in ["acme", "globex"] {
        let response = stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles/1",
                &support::claims_for(tenant),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tenant}");
    }

    // Simulate the consumer-side failure ADR 0018 already handles on the
    // consumer's own side: bytes that arrived some way other than a
    // publication -- a torn mount, not a publisher that would have refused
    // this. `refresh_now` only notifies; the loop's own next iteration is
    // what actually re-reads the file.
    stack.dir.write_raw("tenants.json", b"not json at all");
    stack.handles.tenants.refresh_now();

    support::hold_across(Duration::from_millis(500), || async {
        for (tenant, title) in [("acme", "Acme Handbook"), ("globex", "Globex Playbook")] {
            let response = stack
                .app
                .clone()
                .oneshot(support::request(
                    "GET",
                    "/articles/1",
                    &support::claims_for(tenant),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{tenant}");
            let body = support::body_json(response).await;
            assert_eq!(body["title"], title, "{tenant}");
        }
    })
    .await;
}

#[tokio::test]
async fn a_malformed_published_document_does_not_deprovision_the_tenants_already_serving() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    for tenant in ["acme", "globex"] {
        let response = stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles/1",
                &support::claims_for(tenant),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tenant}");
    }

    stack.dir.write_raw("data-sources.json", b"{not even an array}");
    stack.handles.data_sources.refresh_now();

    support::hold_across(Duration::from_millis(500), || async {
        for (tenant, title) in [("acme", "Acme Handbook"), ("globex", "Globex Playbook")] {
            let response = stack
                .app
                .clone()
                .oneshot(support::request(
                    "GET",
                    "/articles/1",
                    &support::claims_for(tenant),
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{tenant}");
            let body = support::body_json(response).await;
            assert_eq!(body["title"], title, "{tenant}");
        }
    })
    .await;
}

#[tokio::test]
async fn an_emptying_publication_is_refused_unless_it_is_intended() {
    let stack = support::build_stack(&support::base_snapshot(1)).await;

    let mut refused = support::base_snapshot(1);
    refused.tenants = DocumentInput::new(DocumentRevision::new(2), vec![]);

    let error = stack.dir.publisher().publish(&refused).await.unwrap_err();
    assert!(
        matches!(error, PublicationError::EmptyingNotIntended { .. }),
        "{error}"
    );

    // The guard, not luck, is what kept both tenants published.
    for tenant in ["acme", "globex"] {
        let response = stack
            .app
            .clone()
            .oneshot(support::request(
                "GET",
                "/articles/1",
                &support::claims_for(tenant),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{tenant}");
    }

    let mut intended = support::base_snapshot(1);
    intended.tenants = DocumentInput::new(DocumentRevision::new(2), vec![]).emptying_intended();

    let report = stack.dir.publisher().publish(&intended).await.unwrap();
    assert_eq!(report.tenants, DocumentOutcome::Written);

    stack.handles.tenants.refresh_now();

    // Proof that the guard was what stood between a bad sweep and a mass
    // deprovision: once the emptying publication is actually held and the
    // runtime has refreshed, both tenants fail closed at 403 -- an unknown
    // tenant, not a crash and not a silent 200 over nothing.
    for tenant in ["acme", "globex"] {
        support::poll_for_status(
            &stack.app,
            StatusCode::FORBIDDEN,
            || support::request("GET", "/articles/1", &support::claims_for(tenant)),
            Duration::from_secs(2),
        )
        .await;
    }
}
