//! The fixtures a client's lifecycle is driven through: a client with one
//! logical data source, a data source to place it on, a publication target,
//! and an application with a resource so a pass has something to publish.

// Reading a JSON field is the assertion here; see `mod.rs`.
#![allow(clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::Router;
use fabric_control_plane::PublicationSink;
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDeclaration, IsolationModelDocument, PlacementClassDocument,
    PlacementRecord, PoolSettingsDocument,
};
use fabric_runtime_publication::FilesystemRuntimePublication;
use http::StatusCode;
use serde_json::{json, Value};

use super::{as_operator, json as body_of, send, TempDir};

pub const DATA_SOURCE: &str = "shared-postgres-nz-01";

/// A client that asks for one logical data source, so provisioning has
/// something to be met by.
pub const GLOBEX: &str = r"
apiVersion: fabric.fieldstate.nz/v2
kind: Client
metadata:
  name: globex
spec:
  displayName: Globex
  identity:
    realm: globex
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
  data:
    primary:
      class: shared
";

pub fn sink(dir: &TempDir) -> PublicationSink {
    PublicationSink {
        target: Arc::new(FilesystemRuntimePublication::new(
            dir.path().join("tenants.json"),
            dir.path().join("data-sources.json"),
            dir.path().join("catalog.json"),
        )),
        issuer_origin: None,
        readers: None,
    }
}

pub fn declared_data_source() -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").unwrap(),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").unwrap(),
        },
        placement: PlacementClassDocument::Shared,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: Some("NZ".to_owned()),
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: Some(fabric_platform_management::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
        }),
        labels: BTreeMap::new(),
    }
}

pub fn placement(tenant: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new(DATA_SOURCE).unwrap(),
        isolation: IsolationModelDocument::Discriminator {
            column: fabric_platform_management::FieldName::try_new("tenant_key").unwrap(),
            value: format!("{tenant}-key"),
        },
        placed_at: "2026-10-11T00:00:00Z".to_owned(),
    }
}

pub async fn publish_one_application_with_a_resource(router: &Router) {
    async fn command(router: &Router, revision: &Value, value: &Value) -> Value {
        let builder = as_operator("POST", "/api/catalogue").header("content-type", "application/json");
        let builder = if let Some(revision) = revision.as_str() {
            builder.header("if-match", format!("\"{revision}\""))
        } else {
            builder.header("if-none-match", "*")
        };
        let response = send(router, builder.body(Body::from(value.to_string())).unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        body_of(response).await
    }

    // Over whatever catalogue is already there, or as the first write.
    let (_, current) = get(router, "/api/catalogue").await;
    let created = command(
        router,
        &current["revision"],
        &json!({"action":"createApplication","id":"workspec","name":"Workspec"}),
    )
    .await;
    let definition = json!({
        "name": "Workspec", "description": "", "domain": "{client}.example.com",
        "components": [], "features": [],
        "plans": [{"id":"standard","name":"Standard","description":"","features":[],"configuration":{}}],
        "fields": [], "navigation": [],
        "resources": [{
            "name": "customers", "dataSource": "primary", "collection": "customers", "keyField": "id",
            "operations": ["read", "list"], "queryableFields": [],
        }],
    });
    let saved = command(
        router,
        &created["revision"],
        &json!({"action":"saveApplication","id":"workspec","definition":definition}),
    )
    .await;
    command(
        router,
        &saved["revision"],
        &json!({"action":"publishApplication","id":"workspec","note":"v1"}),
    )
    .await;
}

pub async fn get(router: &Router, path: &str) -> (StatusCode, Value) {
    let response = send(router, as_operator("GET", path).body(Body::empty()).unwrap()).await;
    let status = response.status();
    (status, body_of(response).await)
}

pub async fn post(router: &Router, path: &str, revision: Option<&str>, body: &Value) -> (StatusCode, Value) {
    let mut builder = as_operator("POST", path).header("content-type", "application/json");
    if let Some(revision) = revision {
        builder = builder.header("if-match", format!("\"{revision}\""));
    }
    let response = send(router, builder.body(Body::from(body.to_string())).unwrap()).await;
    let status = response.status();
    (status, body_of(response).await)
}

pub async fn tombstone(router: &Router, client: &str, revision: &str) -> (StatusCode, Value) {
    post(
        router,
        &format!("/api/clients/{client}/tombstone"),
        Some(revision),
        &json!({"reason": "Contract ended"}),
    )
    .await
}

pub async fn confirm_deletion(router: &Router, client: &str, revision: &str) -> (StatusCode, Value) {
    post(
        router,
        &format!("/api/clients/{client}/deletion-confirmation"),
        Some(revision),
        &json!({"confirmClientId": client}),
    )
    .await
}

pub fn conditions(lifecycle: &Value) -> Vec<&str> {
    lifecycle["unmet"]
        .as_array()
        .unwrap()
        .iter()
        .map(|unmet| unmet["condition"].as_str().unwrap())
        .collect()
}

pub fn tenants_document(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join("tenants.json")).unwrap()
}

pub fn tenants_published(dir: &TempDir) -> Vec<String> {
    let tenants: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("tenants.json")).unwrap()).unwrap();
    tenants
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["tenant"].as_str().unwrap().to_owned())
        .collect()
}
