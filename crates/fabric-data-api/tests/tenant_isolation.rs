//! Isolation, driven through the assembled router.
//!
//! The most important file in this suite. Under discriminator isolation the
//! tenant boundary exists *only* because the platform adds a predicate, and a
//! missing predicate does not raise an error — it returns other tenants' rows
//! with a 200.

mod support;

use fabric_connector::{ComparisonOperator, Filter, MutationSpec};
use http::StatusCode;
use serde_json::{json, Value};
use support::{app, field, json_request, request};
use tower::ServiceExt as _;

/// The predicate `globex`'s binding must produce.
fn tenant_predicate() -> Filter {
    Filter::Compare {
        field: field("tenant_key"),
        operator: ComparisonOperator::Equal,
        value: Value::String("tenant-482".to_owned()),
    }
}

#[tokio::test]
async fn a_shared_data_source_query_carries_the_tenant_predicate() {
    // Without this, globex's list returns every tenant's rows with a 200.
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    assert_eq!(connector.last_query().1.filter, Some(tenant_predicate()));
}

#[tokio::test]
async fn a_caller_filter_cannot_displace_the_tenant_predicate() {
    let (app, connector) = app();

    app.oneshot(request(
        "GET",
        "/customers?tenant_key=tenant-999",
        json!({"tenant_id": "globex"}),
    ))
    .await
    .unwrap();

    let Some(Filter::And { clauses }) = connector.last_query().1.filter else {
        panic!("the caller filter must be conjoined with the tenant predicate");
    };

    // Both survive, so the conjunction can only narrow — never widen.
    assert!(clauses.contains(&tenant_predicate()));
}

#[tokio::test]
async fn a_dedicated_data_source_query_needs_no_predicate() {
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    // Isolation is structural here — the connection cannot see other tenants.
    assert_eq!(connector.last_query().1.filter, None);
}

#[tokio::test]
async fn a_created_record_is_stamped_with_the_tenant_discriminator() {
    let (app, connector) = app();

    let response = app
        .oneshot(json_request(
            "POST",
            "/customers",
            json!({"tenant_id": "globex"}),
            &json!({"name": "Alice", "tenant_key": "tenant-999"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let MutationSpec::Insert { rows, .. } = connector.last_mutation().1 else {
        panic!("expected an insert");
    };

    // The caller's hostile value is overwritten, not merged.
    assert_eq!(
        rows.first().unwrap().get(&field("tenant_key")),
        Some(&Value::String("tenant-482".to_owned()))
    );
}

#[tokio::test]
async fn an_update_cannot_move_a_record_to_another_tenant() {
    let (app, connector) = app();

    app.oneshot(json_request(
        "PATCH",
        "/customers/1",
        json!({"tenant_id": "globex"}),
        &json!({"tenant_key": "tenant-999"}),
    ))
    .await
    .unwrap();

    let MutationSpec::Update { changes, filter, .. } = connector.last_mutation().1 else {
        panic!("expected an update");
    };

    assert_eq!(
        changes.get(&field("tenant_key")),
        Some(&Value::String("tenant-482".to_owned()))
    );
    assert!(filter.is_some());
}

#[tokio::test]
async fn a_delete_is_scoped_to_the_tenant_as_well_as_the_key() {
    let (app, connector) = app();

    app.oneshot(request("DELETE", "/customers/42", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    let MutationSpec::Delete { filter, .. } = connector.last_mutation().1 else {
        panic!("expected a delete");
    };

    let Some(Filter::And { clauses }) = filter else {
        panic!("a delete must carry both the key and the tenant predicate");
    };
    assert_eq!(clauses.len(), 2);
    assert!(clauses.contains(&tenant_predicate()));
}
