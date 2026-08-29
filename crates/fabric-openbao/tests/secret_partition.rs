//! The instance's secret partition, against a store that answers over a socket.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use fabric_control_plane::{GitIntegration, IntegrationStore, SecretName, SecretStore, SecretValue};
use fabric_core::SystemClock;
use fabric_openbao::{OpenBao, OpenBaoConfig, OpenBaoIntegrationStore, OpenBaoSecretStore};
use support::{FakeOpenBao, Recorded};

/// A service-account token on disk, which is what the pod would have.
fn token_file() -> std::path::PathBuf {
    let path = std::env::temp_dir().join("fabric-openbao-test-token");
    std::fs::write(&path, "a-service-account-jwt").unwrap();
    path
}

fn config(address: &str) -> OpenBaoConfig {
    serde_json::from_value(serde_json::json!({
        "address": address,
        "role": "saas-fabric-control-plane",
        "service_account_token_path": token_file().to_string_lossy(),
    }))
    .unwrap()
}

fn client(fake: &FakeOpenBao) -> Arc<OpenBao> {
    Arc::new(OpenBao::new(&config(&fake.address), SystemClock::shared()).unwrap())
}

/// How the fake answers a data or metadata request.
type Responder = Arc<dyn Fn(&Recorded) -> (u16, String) + Send + Sync>;

/// A store that answers every request the same way.
fn answering(status: u16, body: &'static str) -> Responder {
    Arc::new(move |_| (status, body.to_owned()))
}

#[tokio::test]
async fn a_secret_is_read_out_of_a_version_two_entry() {
    // Version 2 nests twice: the envelope's `data` holds version metadata, and
    // *its* `data` holds what was written. Reading one level is the classic
    // mistake and produces a very confusing "malformed".
    let fake = FakeOpenBao::start(answering(
        200,
        r#"{"data":{"data":{"value":"the-private-key"},"metadata":{"version":1}}}"#,
    ))
    .await;

    let store = OpenBaoSecretStore::new(client(&fake));

    let read = store.get(&SecretName::new("git/app-private-key")).await.unwrap();

    assert_eq!(
        read.map(|value| value.expose().to_owned()),
        Some("the-private-key".to_owned())
    );
}

#[tokio::test]
async fn the_adapter_logs_in_before_it_reads_and_presents_the_token() {
    let fake = FakeOpenBao::start(answering(200, r#"{"data":{"data":{"value":"x"}}}"#)).await;

    OpenBaoSecretStore::new(client(&fake))
        .get(&SecretName::new("git/app-private-key"))
        .await
        .unwrap();

    assert_eq!(fake.logins(), 1, "the pod's own identity is exchanged first");
    assert_eq!(
        fake.requests()[0].token.as_deref(),
        Some("a-store-token"),
        "and the resulting token is presented in OpenBao's own header"
    );
}

#[tokio::test]
async fn a_secret_that_has_never_been_written_is_absent_rather_than_an_error() {
    // A platform that has never connected has no key, and that is an ordinary
    // state — reporting it as a failure would make a fresh install look broken.
    let fake = FakeOpenBao::start(answering(404, r#"{"errors":[]}"#)).await;

    let read = OpenBaoSecretStore::new(client(&fake))
        .get(&SecretName::new("git/app-private-key"))
        .await
        .unwrap();

    assert!(read.is_none());
}

#[tokio::test]
async fn a_secret_is_written_beneath_this_instances_partition() {
    let fake = FakeOpenBao::start(answering(200, "{}")).await;

    OpenBaoSecretStore::new(client(&fake))
        .put(
            &SecretName::new("git/app-private-key"),
            &SecretValue::new("the-private-key"),
        )
        .await
        .unwrap();

    let request = &fake.requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(
        request.path, "/v1/secret/data/platform/saas-fabric/instances/master/git/app-private-key",
        "a name is resolved beneath the instance's prefix, which no caller supplies"
    );
    assert!(request.body.contains("the-private-key"));
}

#[tokio::test]
async fn deleting_removes_every_version_rather_than_marking_the_latest_deleted() {
    // Deleting through `data` leaves previous versions readable, which for a
    // private key is not deletion at all.
    let fake = FakeOpenBao::start(answering(204, "")).await;

    OpenBaoSecretStore::new(client(&fake))
        .delete(&SecretName::new("git/app-private-key"))
        .await
        .unwrap();

    let request = &fake.requests()[0];
    assert_eq!(request.method, "DELETE");
    assert!(
        request.path.contains("/metadata/"),
        "must delete through metadata, not data: {}",
        request.path
    );
}

#[tokio::test]
async fn a_refused_token_is_replaced_and_the_request_retried_once() {
    // A store token can be revoked before its lease is up, and the platform
    // learns that as a 403 on an ordinary call rather than as an expiry.
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);

    let responder: Responder = Arc::new(move |_: &Recorded| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            (403, r#"{"errors":["permission denied"]}"#.to_owned())
        } else {
            (200, r#"{"data":{"data":{"value":"the-private-key"}}}"#.to_owned())
        }
    });

    let fake = FakeOpenBao::start(responder).await;

    let read = OpenBaoSecretStore::new(client(&fake))
        .get(&SecretName::new("git/app-private-key"))
        .await
        .unwrap();

    assert!(read.is_some(), "the retry must succeed");
    assert_eq!(
        fake.logins(),
        2,
        "and it must log in again rather than reuse the refused token"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2, "exactly once, not a loop");
}

#[tokio::test]
async fn the_integration_record_round_trips_through_the_store() {
    let fake = FakeOpenBao::start(answering(200, "{}")).await;
    let store = OpenBaoIntegrationStore::new(client(&fake));

    store
        .save(&GitIntegration::created("1234", "saas-fabric"))
        .await
        .unwrap();

    let written = &fake.requests()[0];
    assert!(written.path.ends_with("/git/integration"));
    assert!(written.body.contains("saas-fabric"));
}

#[tokio::test]
async fn no_integration_record_is_absence_rather_than_a_failure() {
    let fake = FakeOpenBao::start(answering(404, r#"{"errors":[]}"#)).await;

    let loaded = OpenBaoIntegrationStore::new(client(&fake)).load().await.unwrap();

    assert!(loaded.is_none());
}
