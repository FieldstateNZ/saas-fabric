//! The image registry records, and a registry's credential, beneath the
//! instance's partition (ADR 0026 section 5).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::Arc;

use fabric_control_plane::{
    RegistryRecord, RegistryStore, RegistryStoreError, SecretName, SecretStore, SecretValue,
};
use fabric_core::SystemClock;
use fabric_openbao::{OpenBao, OpenBaoConfig, OpenBaoRegistryStore, OpenBaoSecretStore};
use support::{FakeOpenBao, Recorded};

/// Where every name lands: the instance's partition, which no caller names.
const PARTITION: &str = "/v1/secret/data/platform/saas-fabric/instances/master";

fn client(fake: &FakeOpenBao) -> Arc<OpenBao> {
    let token = std::env::temp_dir().join("fabric-openbao-registry-test-token");
    std::fs::write(&token, "a-service-account-jwt").unwrap();
    let config: OpenBaoConfig = serde_json::from_value(serde_json::json!({
        "address": fake.address,
        "role": "saas-fabric-control-plane",
        "service_account_token_path": token.to_string_lossy(),
    }))
    .unwrap();
    Arc::new(OpenBao::new(&config, SystemClock::shared()).unwrap())
}

type Responder = Arc<dyn Fn(&Recorded) -> (u16, String) + Send + Sync>;

fn answering(status: u16, body: String) -> Responder {
    Arc::new(move |_| (status, body.clone()))
}

fn record() -> RegistryRecord {
    serde_json::from_value(serde_json::json!({
        "host": "ghcr.io",
        "kind": "ghcr",
        "endpoint": "https://ghcr.io",
        "secretId": "0123456789abcdef",
        "credential": {"username": "registry-robot", "setBy": "brett", "setAt": 1},
        "registeredBy": "brett",
        "registeredAt": 1,
        "repositories": [{"repository": "ghcr.io/acme/app", "provenAt": 1}],
    }))
    .unwrap()
}

#[tokio::test]
async fn the_record_set_is_written_beneath_this_instances_partition_as_one_entry() {
    let fake = FakeOpenBao::start(answering(200, "{}".to_owned())).await;

    OpenBaoRegistryStore::new(client(&fake))
        .save(&[record()])
        .await
        .unwrap();

    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, format!("{PARTITION}/integrations/registries"));
    assert!(requests[0].body.contains(r#""record""#), "{}", requests[0].body);
    assert!(
        requests[0].body.contains("ghcr.io/acme/app"),
        "{}",
        requests[0].body
    );
}

#[tokio::test]
async fn the_record_set_round_trips_out_of_a_version_two_entry() {
    let stored = serde_json::to_string(&vec![record()]).unwrap();
    let body = serde_json::json!({"data": {"data": {"record": stored}}}).to_string();
    let fake = FakeOpenBao::start(answering(200, body)).await;

    let loaded = OpenBaoRegistryStore::new(client(&fake)).load().await.unwrap();

    assert_eq!(loaded, vec![record()]);
    assert_eq!(
        fake.requests()[0].path,
        format!("{PARTITION}/integrations/registries")
    );
}

#[tokio::test]
async fn nothing_recorded_is_an_empty_set_rather_than_a_failure() {
    let fake = FakeOpenBao::start(answering(404, r#"{"errors":[]}"#.to_owned())).await;

    let loaded = OpenBaoRegistryStore::new(client(&fake)).load().await.unwrap();

    assert!(loaded.is_empty());
}

#[tokio::test]
async fn a_record_set_that_will_not_parse_is_malformed_rather_than_empty() {
    let body = serde_json::json!({"data": {"data": {"record": "[{\"host\":\"ghcr.io\"}]"}}}).to_string();
    let fake = FakeOpenBao::start(answering(200, body)).await;

    let loaded = OpenBaoRegistryStore::new(client(&fake)).load().await;

    assert_eq!(loaded, Err(RegistryStoreError::Malformed));
}

#[tokio::test]
async fn a_registrys_credential_is_its_own_secret_beneath_the_partition() {
    let fake = FakeOpenBao::start(answering(200, "{}".to_owned())).await;

    OpenBaoSecretStore::new(client(&fake))
        .put(
            &SecretName::new("integrations/registries/0123456789abcdef/credential"),
            &SecretValue::new("ghp_a-token"),
        )
        .await
        .unwrap();

    let requests = fake.requests();
    assert_eq!(
        requests[0].path,
        format!("{PARTITION}/integrations/registries/0123456789abcdef/credential")
    );
}
