//! End-to-end tests for the Data API, driving the real router.
//!
//! These exist to prove the property the whole platform rests on: **the tenant
//! comes from the bearer token, and there is no way for a caller to change
//! it.** Unit tests can show each link in the chain works; only driving the
//! assembled router can show the chain is actually connected.
//!
//! The connector is a recording stub rather than a real database. That is
//! deliberate — what is under test here is tenant resolution, scoping, and
//! error mapping, and a real database would add setup cost without testing any
//! of it. What *reaches* the connector is exactly what the test asserts on.

// `clippy.toml`'s `allow-unwrap-in-tests` only covers `#[cfg(test)]` modules;
// an integration test is its own crate, so the allowances are declared here.
// Panicking on a bad fixture is the correct behaviour in a test.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use axum::body::Body;
use axum::Router;
use fabric_connector::{
    CollectionName, CollectionSchema, ComparisonOperator, ConnectionName, ConnectionSelector,
    ConnectorCapabilities, ConnectorError, ConnectorId, ConnectorRegistry, ConnectorSchema, DataConnector,
    ExecutionTarget, FieldName, Filter, IsolationModel, MutationOutcome, MutationSpec, QueryOutcome,
    QuerySpec, Row,
};
use fabric_core::{BindingRevision, Clock, DataSourceName, TenantId};
use fabric_data_api::{build_data_api, DataApiConfig, ResourceCatalog, ResourcePermissions};
use fabric_identity::{build_identity, encode_unsigned_token, IdentityConfig, TrustedIngressReader};
use fabric_tenant_runtime::{DataBinding, TenantRuntimeBinding, TenantRuntimeRegistry};
use http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt as _;

// ---------------------------------------------------------------- test doubles

/// A clock frozen so unsigned test tokens never expire.
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_000
    }
}

/// What the connector was asked to do.
#[derive(Default)]
struct Recorded {
    queries: Vec<(ExecutionTarget, QuerySpec)>,
    mutations: Vec<(ExecutionTarget, MutationSpec)>,
}

/// A connector that records what it receives and returns canned rows.
struct RecordingConnector {
    id: ConnectorId,
    capabilities: ConnectorCapabilities,
    schema: ConnectorSchema,
    recorded: Mutex<Recorded>,
    rows: Vec<Row>,
}

impl RecordingConnector {
    fn new(rows: Vec<Row>) -> Arc<Self> {
        let capabilities = ConnectorCapabilities {
            mutations: true,
            ..ConnectorCapabilities::baseline()
        };

        let schema = ConnectorSchema::new([(
            CollectionName::try_new("customers").unwrap(),
            CollectionSchema::new([field("id"), field("name"), field("tenant_key")]),
        )]);

        Arc::new(Self {
            id: ConnectorId::try_new("postgres").unwrap(),
            capabilities,
            schema,
            recorded: Mutex::new(Recorded::default()),
            rows,
        })
    }

    fn last_query(&self) -> (ExecutionTarget, QuerySpec) {
        self.recorded
            .lock()
            .unwrap()
            .queries
            .last()
            .cloned()
            .expect("the connector received no query")
    }

    fn last_mutation(&self) -> (ExecutionTarget, MutationSpec) {
        self.recorded
            .lock()
            .unwrap()
            .mutations
            .last()
            .cloned()
            .expect("the connector received no mutation")
    }

    fn query_count(&self) -> usize {
        self.recorded.lock().unwrap().queries.len()
    }
}

#[async_trait]
impl DataConnector for RecordingConnector {
    fn id(&self) -> &ConnectorId {
        &self.id
    }

    fn capabilities(&self) -> &ConnectorCapabilities {
        &self.capabilities
    }

    fn schema(&self) -> &ConnectorSchema {
        &self.schema
    }

    async fn query(
        &self,
        target: &ExecutionTarget,
        spec: &QuerySpec,
    ) -> Result<QueryOutcome, ConnectorError> {
        self.recorded
            .lock()
            .unwrap()
            .queries
            .push((target.clone(), spec.clone()));

        Ok(QueryOutcome::from_rows(self.rows.clone()))
    }

    async fn mutate(
        &self,
        target: &ExecutionTarget,
        spec: &MutationSpec,
    ) -> Result<MutationOutcome, ConnectorError> {
        self.recorded
            .lock()
            .unwrap()
            .mutations
            .push((target.clone(), spec.clone()));

        Ok(MutationOutcome::affected(1))
    }

    async fn health(&self) -> Result<(), ConnectorError> {
        Ok(())
    }
}

// ------------------------------------------------------------------- fixtures

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}

fn tenant(name: &str) -> TenantId {
    TenantId::try_new(name).unwrap()
}

fn row(id: i64, name: &str) -> Row {
    Row::new()
        .with(field("id"), Value::from(id))
        .with(field("name"), Value::String(name.to_owned()))
}

/// `acme` has a dedicated database; `globex` shares a table with a
/// discriminator. Two placements, one API — which is the point of §18.
fn bindings() -> Vec<TenantRuntimeBinding> {
    let primary = DataSourceName::try_new("primary").unwrap();

    let acme = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(7)).with_data(
        primary.clone(),
        DataBinding {
            connector: ConnectorId::try_new("postgres").unwrap(),
            connection: ConnectionSelector::Named {
                name: ConnectionName::try_new("acme-prod").unwrap(),
            },
            isolation: IsolationModel::Database,
        },
    );

    let globex = TenantRuntimeBinding::new(tenant("globex"), BindingRevision::new(3)).with_data(
        primary,
        DataBinding {
            connector: ConnectorId::try_new("postgres").unwrap(),
            connection: ConnectionSelector::Named {
                name: ConnectionName::try_new("shared-02").unwrap(),
            },
            isolation: IsolationModel::Discriminator {
                column: field("tenant_key"),
                value: "tenant-482".to_owned(),
            },
        },
    );

    vec![acme, globex]
}

fn catalog() -> ResourceCatalog {
    serde_json::from_str(
        r#"{
            "customers": {
                "data_source": "primary",
                "collection": "customers",
                "operations": ["read", "list", "create", "update", "delete"]
            },
            "readOnlyReport": {
                "data_source": "primary",
                "collection": "customers"
            }
        }"#,
    )
    .unwrap()
}

/// Builds the assembled router, plus the connector so tests can inspect it.
fn app_with(registry: Arc<TenantRuntimeRegistry>) -> (Router, Arc<RecordingConnector>) {
    let connector = RecordingConnector::new(vec![row(1, "Alice"), row(2, "Bob")]);

    let identity = build_identity(
        IdentityConfig::default(),
        Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
    )
    .unwrap();

    let router = build_data_api(
        &DataApiConfig::default(),
        catalog(),
        // Scope checks are exercised separately; most tests are about tenancy.
        ResourcePermissions {
            require_scopes: false,
            ..ResourcePermissions::default()
        },
        registry,
        ConnectorRegistry::new().with(Arc::clone(&connector) as Arc<dyn DataConnector>),
        identity,
    )
    .unwrap();

    (router, connector)
}

fn app() -> (Router, Arc<RecordingConnector>) {
    let registry = Arc::new(TenantRuntimeRegistry::new());
    registry.apply_all(bindings());

    app_with(registry)
}

fn token_for(claims: Value) -> String {
    let Value::Object(object) = claims else {
        panic!("claims must be an object");
    };

    encode_unsigned_token(&object)
}

fn request(method: &str, uri: &str, claims: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .body(Body::empty())
        .unwrap()
}

fn json_request(method: &str, uri: &str, claims: Value, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", token_for(claims)))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();

    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

// --------------------------------------------------------------- happy paths

#[tokio::test]
async fn lists_records_for_the_tenant_in_the_token() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let (target, _) = connector.last_query();
    assert_eq!(target.tenant(), &tenant("acme"));
    assert_eq!(target.revision(), BindingRevision::new(7));

    let body = body_json(response).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"][0]["name"], "Alice");
}

#[tokio::test]
async fn the_application_never_names_a_database_but_one_is_selected_for_it() {
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();

    assert_eq!(
        target.connection(),
        &ConnectionSelector::Named {
            name: ConnectionName::try_new("acme-prod").unwrap()
        }
    );
}

#[tokio::test]
async fn the_same_url_reaches_a_different_database_for_a_different_tenant() {
    // §16: one application contract, different physical placement per tenant.
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();

    assert_eq!(
        target.connection(),
        &ConnectionSelector::Named {
            name: ConnectionName::try_new("shared-02").unwrap()
        }
    );
}

// ------------------------------------------------------------ tenant scoping

#[tokio::test]
async fn a_shared_table_query_carries_the_tenant_predicate() {
    // The single most important assertion in this file. Without this predicate,
    // globex's list returns every tenant's rows with a 200.
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    let (_, spec) = connector.last_query();

    let expected = Filter::Compare {
        field: field("tenant_key"),
        operator: ComparisonOperator::Equal,
        value: Value::String("tenant-482".to_owned()),
    };

    assert_eq!(spec.filter, Some(expected));
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

    let (_, spec) = connector.last_query();

    let Some(Filter::And { clauses }) = spec.filter else {
        panic!("the caller filter must be conjoined with the tenant predicate");
    };

    // Both survive, so the conjunction can only narrow — never widen.
    assert!(clauses.contains(&Filter::Compare {
        field: field("tenant_key"),
        operator: ComparisonOperator::Equal,
        value: Value::String("tenant-482".to_owned()),
    }));
}

#[tokio::test]
async fn a_dedicated_database_query_needs_no_predicate() {
    let (app, connector) = app();

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (_, spec) = connector.last_query();

    // Isolation is structural here — the connection cannot see other tenants.
    assert_eq!(spec.filter, None);
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

    let (_, spec) = connector.last_mutation();
    let MutationSpec::Insert { rows, .. } = spec else {
        panic!("expected an insert");
    };

    // The caller's hostile value is overwritten, not merged.
    assert_eq!(
        rows.first().unwrap().get(&field("tenant_key")),
        Some(&Value::String("tenant-482".to_owned()))
    );
}

#[tokio::test]
async fn a_delete_is_scoped_to_the_tenant_as_well_as_the_key() {
    let (app, connector) = app();

    app.oneshot(request("DELETE", "/customers/42", json!({"tenant_id": "globex"})))
        .await
        .unwrap();

    let (_, spec) = connector.last_mutation();
    let MutationSpec::Delete { filter, .. } = spec else {
        panic!("expected a delete");
    };

    let Some(Filter::And { clauses }) = filter else {
        panic!("a delete must carry both the key and the tenant predicate");
    };
    assert_eq!(clauses.len(), 2);
}

// -------------------------------------------------------- the tenant is fixed

#[tokio::test]
async fn a_tenant_header_is_rejected_outright() {
    // §11: there must be exactly one authoritative tenant context.
    let (app, _) = app();

    let request = Request::builder()
        .method("GET")
        .uri("/customers")
        .header(
            "authorization",
            format!("Bearer {}", token_for(json!({"tenant_id": "acme"}))),
        )
        .header("x-tenant-id", "globex")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_request_with_no_token_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(Request::builder().uri("/customers").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0, "no query may reach a connector");
}

#[tokio::test]
async fn a_token_with_no_tenant_claim_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"sub": "user-123"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(connector.query_count(), 0);
}

// ---------------------------------------------------------------- failing closed

#[tokio::test]
async fn an_unknown_tenant_is_refused_without_reaching_a_connector() {
    let (app, connector) = app();

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "ghost"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(connector.query_count(), 0);

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "unknown_tenant");
    // The tenant is not echoed back.
    assert!(!body["error"]["message"].as_str().unwrap().contains("ghost"));
}

#[tokio::test]
async fn an_unprimed_runtime_returns_service_unavailable_not_forbidden() {
    // §28: a cold start must not tell every caller their tenant is gone.
    let (app, _) = app_with(Arc::new(TenantRuntimeRegistry::new()));

    let response = app
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["error"]["code"], "runtime_unavailable");
}

#[tokio::test]
async fn an_uncatalogued_resource_is_a_404() {
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/invoices", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_operation_the_catalogue_does_not_expose_is_refused() {
    let (app, connector) = app();

    let response = app
        .oneshot(json_request(
            "POST",
            "/readOnlyReport",
            json!({"tenant_id": "acme"}),
            &json!({"name": "Alice"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(connector.query_count(), 0);
}

#[tokio::test]
async fn a_scope_check_refuses_an_unauthorised_operation() {
    let registry = Arc::new(TenantRuntimeRegistry::new());
    registry.apply_all(bindings());

    let connector = RecordingConnector::new(vec![]);
    let identity = build_identity(
        IdentityConfig::default(),
        Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
    )
    .unwrap();

    let app = build_data_api(
        &DataApiConfig::default(),
        catalog(),
        ResourcePermissions::default(),
        registry,
        ConnectorRegistry::new().with(Arc::clone(&connector) as Arc<dyn DataConnector>),
        identity,
    )
    .unwrap();

    let response = app
        .oneshot(request(
            "DELETE",
            "/customers/1",
            json!({"tenant_id": "acme", "scope": "data:customers:read"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ------------------------------------------------------------------- querying

#[tokio::test]
async fn paging_asks_for_one_row_beyond_the_page() {
    let (app, connector) = app();

    app.oneshot(request(
        "GET",
        "/customers?limit=1&offset=10",
        json!({"tenant_id": "acme"}),
    ))
    .await
    .unwrap();

    let (_, spec) = connector.last_query();

    // The probe row is what makes `has_more` a fact.
    assert_eq!(spec.limit, Some(2));
    assert_eq!(spec.offset, Some(10));
}

#[tokio::test]
async fn a_full_page_reports_that_more_records_exist() {
    let (app, _) = app();

    let response = app
        .oneshot(request("GET", "/customers?limit=1", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let body = body_json(response).await;

    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["paging"]["has_more"], true);
}

#[tokio::test]
async fn an_excessive_limit_is_clamped_rather_than_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers?limit=999999",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let (_, spec) = connector.last_query();
    assert_eq!(spec.limit, Some(1001), "clamped to max_limit plus the probe row");
}

#[tokio::test]
async fn a_read_by_key_that_matches_nothing_is_a_404() {
    let registry = Arc::new(TenantRuntimeRegistry::new());
    registry.apply_all(bindings());

    let connector = RecordingConnector::new(vec![]);
    let identity = build_identity(
        IdentityConfig::default(),
        Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
    )
    .unwrap();

    let app = build_data_api(
        &DataApiConfig::default(),
        catalog(),
        ResourcePermissions {
            require_scopes: false,
            ..ResourcePermissions::default()
        },
        registry,
        ConnectorRegistry::new().with(Arc::clone(&connector) as Arc<dyn DataConnector>),
        identity,
    )
    .unwrap();

    let response = app
        .oneshot(request("GET", "/customers/999", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_invalid_field_name_in_a_filter_is_rejected() {
    let (app, connector) = app();

    let response = app
        .oneshot(request(
            "GET",
            "/customers?drop%20table=1",
            json!({"tenant_id": "acme"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(connector.query_count(), 0);
}

// -------------------------------------------------------------- live migration

#[tokio::test]
async fn changing_a_binding_moves_the_tenant_without_redeploying_anything() {
    // §19: tenant resources change; the application keeps calling /customers.
    let registry = Arc::new(TenantRuntimeRegistry::new());
    registry.apply_all(bindings());

    let (app, connector) = app_with(Arc::clone(&registry));

    app.clone()
        .oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    assert_eq!(
        connector.last_query().0.connection(),
        &ConnectionSelector::Named {
            name: ConnectionName::try_new("acme-prod").unwrap()
        }
    );

    // Reconciliation publishes a new revision pointing somewhere else.
    let moved = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(8)).with_data(
        DataSourceName::try_new("primary").unwrap(),
        DataBinding {
            connector: ConnectorId::try_new("postgres").unwrap(),
            connection: ConnectionSelector::Named {
                name: ConnectionName::try_new("acme-db-01").unwrap(),
            },
            isolation: IsolationModel::Database,
        },
    );
    assert!(registry.apply_one(moved));

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();
    assert_eq!(
        target.connection(),
        &ConnectionSelector::Named {
            name: ConnectionName::try_new("acme-db-01").unwrap()
        }
    );
    assert_eq!(target.revision(), BindingRevision::new(8));
}

#[tokio::test]
async fn a_stale_binding_update_is_ignored_by_the_request_path() {
    let registry = Arc::new(TenantRuntimeRegistry::new());
    registry.apply_all(bindings());

    let (app, connector) = app_with(Arc::clone(&registry));

    // An out-of-order update carrying an older revision.
    let stale = TenantRuntimeBinding::new(tenant("acme"), BindingRevision::new(2)).with_data(
        DataSourceName::try_new("primary").unwrap(),
        DataBinding {
            connector: ConnectorId::try_new("postgres").unwrap(),
            connection: ConnectionSelector::Named {
                name: ConnectionName::try_new("retired-db").unwrap(),
            },
            isolation: IsolationModel::Database,
        },
    );
    assert!(!registry.apply_one(stale));

    app.oneshot(request("GET", "/customers", json!({"tenant_id": "acme"})))
        .await
        .unwrap();

    let (target, _) = connector.last_query();
    assert_eq!(target.revision(), BindingRevision::new(7));
}
