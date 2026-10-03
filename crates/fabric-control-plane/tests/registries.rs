//! Image registries, driven through the real routes (ADR 0026 section 5).
//!
//! The registry itself is a fake behind the `RegistryConnector` port — what
//! the adapter does on the wire is `fabric-registry`'s own suite — so these
//! pin what the control plane decides: which requests the rules refuse,
//! which proofs a change waits on, which code each failure answers, what is
//! audited, and that a token appears in no response and no log line.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Once, PoisonError};

use async_trait::async_trait;
use axum::body::Body;
use fabric_control_plane::{
    DeploymentRegistry, InMemoryRegistryStore, InMemorySecretStore, Readability, RegistryClient,
    RegistryConnection, RegistryConnector, RegistryHost, RegistryService, RegistryServiceParts,
    RegistryStore, Repository, SecretName, SecretStore, SecretStoreError, SecretValue,
};
use fabric_platform_management::RegistryError;
use http::{header, StatusCode};
use serde_json::{json, Value};
use support::{as_operator, control_plane_with_registries, send, FixedClock, TestControlPlane};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

/// A token that must appear nowhere but the secret store.
const TOKEN: &str = "ghp_a-token-that-must-never-leak";

/// A second one, for a replacement.
const SECOND_TOKEN: &str = "ghp_a-second-token-that-must-never-leak";

// ---------------------------------------------------------------------------
// A registry, scripted
// ---------------------------------------------------------------------------

/// How the next client a connector builds answers.
#[derive(Clone, Default)]
struct Script {
    /// What proving the registry fails with, if anything.
    prove: Option<RegistryError>,
    /// The realm origin a proof reports.
    realm: Option<String>,
    /// Repositories whose proof is refused.
    unreadable: BTreeSet<String>,
    /// What a tag listing answers.
    tags: Vec<String>,
    /// Whether the client says its realm refused its credential.
    refused: bool,
    /// Repositories whose proof has the realm refuse the credential, marking
    /// it.
    denied: BTreeSet<String>,
}

struct FakeClient {
    script: Script,
    /// The credential's refusal mark, when one is presented.
    mark: Option<Arc<AtomicBool>>,
}

#[async_trait]
impl RegistryClient for FakeClient {
    async fn prove(&self) -> Result<Option<String>, RegistryError> {
        match &self.script.prove {
            Some(error) => Err(error.clone()),
            None => Ok(self.script.realm.clone()),
        }
    }

    async fn prove_repository(&self, repository: &Repository) -> Result<Readability<()>, RegistryError> {
        if let Some(mark) = self
            .mark
            .as_ref()
            .filter(|_| self.script.denied.contains(repository.as_str()))
        {
            mark.store(true, Ordering::SeqCst);
            return Err(RegistryError::Denied {
                detail: "proving a repository: the realm refused this registry's credential".to_owned(),
            });
        }
        if self.script.unreadable.contains(repository.as_str()) {
            return Ok(Readability::NotReadable);
        }
        Ok(Readability::Readable(()))
    }

    async fn version_tags(
        &self,
        _repository: &Repository,
    ) -> Result<Readability<Vec<String>>, RegistryError> {
        Ok(Readability::Readable(self.script.tags.clone()))
    }

    fn credential_refused(&self) -> bool {
        self.script.refused || self.mark.as_ref().is_some_and(|mark| mark.load(Ordering::SeqCst))
    }
}

/// What a connection was built with, without its token.
#[derive(Clone, Debug)]
struct Built {
    host: String,
    credential: bool,
    repositories: Vec<String>,
}

#[derive(Default)]
struct FakeConnector {
    script: Mutex<Script>,
    built: Mutex<Vec<Built>>,
    installed: Mutex<Vec<Vec<String>>>,
}

impl FakeConnector {
    fn script(&self, change: impl FnOnce(&mut Script)) {
        change(&mut self.script.lock().unwrap());
    }

    fn last_built(&self) -> Built {
        self.built
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("something was built")
    }

    fn last_installed(&self) -> Vec<String> {
        self.installed.lock().unwrap().last().cloned().unwrap_or_default()
    }
}

impl RegistryConnector for FakeConnector {
    fn connect(&self, connection: RegistryConnection) -> Result<Arc<dyn RegistryClient>, String> {
        self.built.lock().unwrap().push(Built {
            host: connection.host.to_string(),
            credential: connection.credential.is_some(),
            repositories: connection.repositories.iter().map(ToString::to_string).collect(),
        });
        Ok(Arc::new(FakeClient {
            script: self.script.lock().unwrap().clone(),
            mark: connection
                .credential
                .as_ref()
                .map(fabric_control_plane::RegistryCredential::refusal_mark),
        }))
    }

    fn install(&self, clients: BTreeMap<RegistryHost, Arc<dyn RegistryClient>>) {
        self.installed
            .lock()
            .unwrap()
            .push(clients.keys().map(ToString::to_string).collect());
    }
}

/// A secret store that can be told to fail every read.
#[derive(Default)]
struct Secrets {
    held: InMemorySecretStore,
    fail_reads: AtomicBool,
}

#[async_trait]
impl SecretStore for Secrets {
    async fn get(&self, name: &SecretName) -> Result<Option<SecretValue>, SecretStoreError> {
        if self.fail_reads.load(Ordering::SeqCst) {
            return Err(SecretStoreError::Unavailable);
        }
        self.held.get(name).await
    }

    async fn put(&self, name: &SecretName, value: &SecretValue) -> Result<(), SecretStoreError> {
        self.held.put(name, value).await
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        self.held.delete(name).await
    }

    fn describe(&self) -> String {
        "a test secret store".to_owned()
    }
}

/// A plane, and the fakes behind its registries.
struct Harness {
    plane: TestControlPlane,
    connector: Arc<FakeConnector>,
    secrets: Arc<Secrets>,
    store: Arc<InMemoryRegistryStore>,
}

fn harness(deployment: Option<DeploymentRegistry>) -> Harness {
    over(
        Arc::default(),
        Arc::default(),
        Arc::new(InMemoryRegistryStore::new()),
        deployment,
    )
}

fn over(
    connector: Arc<FakeConnector>,
    secrets: Arc<Secrets>,
    store: Arc<InMemoryRegistryStore>,
    deployment: Option<DeploymentRegistry>,
) -> Harness {
    let service = Arc::new(RegistryService::new(RegistryServiceParts {
        store: Arc::clone(&store) as Arc<dyn RegistryStore>,
        secrets: Arc::clone(&secrets) as Arc<dyn SecretStore>,
        connector: Arc::clone(&connector) as Arc<dyn RegistryConnector>,
        clock: Arc::new(FixedClock),
        deployment,
    }));
    Harness {
        plane: control_plane_with_registries(service),
        connector,
        secrets,
        store,
    }
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// Sends `body` to `path` as an operator, and answers the status, the
/// `Retry-After` header and the body, as text and as JSON.
async fn call(
    plane: &TestControlPlane,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, bool, Value) {
    let request = as_operator(method, path).header(header::CONTENT_TYPE, "application/json");
    let request = match body {
        Some(body) => request.body(Body::from(body.to_string())).unwrap(),
        None => request.body(Body::empty()).unwrap(),
    };
    let response = send(&plane.router, request).await;
    let status = response.status();
    let retry = response.headers().contains_key(header::RETRY_AFTER);
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(
        !text.contains(TOKEN) && !text.contains(SECOND_TOKEN),
        "a response carried a token: {text}"
    );
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, retry, value)
}

fn code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or_default()
}

async fn register(plane: &TestControlPlane, body: Value) -> (StatusCode, bool, Value) {
    call(plane, "POST", "/api/integrations/registries", Some(body)).await
}

// ---------------------------------------------------------------------------
// Capturing what is logged
// ---------------------------------------------------------------------------

/// Events captured on one thread: each one's fields by name.
type Captured = Arc<Mutex<Vec<BTreeMap<String, String>>>>;

thread_local! {
    /// This thread's captured events, while a test has opted in. A
    /// `current_thread` test runtime runs the service's spawned change on
    /// this same thread, so its audit line lands here too.
    static SINK: RefCell<Option<Captured>> = const { RefCell::new(None) };
}

/// Routes each event to the thread that opted in. Installed once, globally,
/// for the reason `startup::tick`'s own capture explains: a scoped default
/// races every other test's callsites over tracing's interest cache.
struct CapturingLayer;

struct Fields(BTreeMap<String, String>);

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        SINK.with(|sink| {
            if let Some(events) = sink.borrow().as_ref() {
                let mut fields = Fields(BTreeMap::new());
                event.record(&mut fields);
                events
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(fields.0);
            }
        });
    }
}

/// Starts capturing this thread's events.
fn capture() -> Captured {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(CapturingLayer))
            .expect("this binary installs the only global subscriber");
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    SINK.with(|sink| *sink.borrow_mut() = Some(Arc::clone(&events)));
    events
}

/// The audit records captured, as `(operation, outcome, host)`.
fn audited(events: &Mutex<Vec<BTreeMap<String, String>>>) -> Vec<(String, String, String)> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.get("event").map(String::as_str) == Some("control_plane.audit.registry"))
        .map(|event| {
            (
                event["operation"].clone(),
                event["outcome"].clone(),
                event["host"].clone(),
            )
        })
        .collect()
}

/// Asserts no captured event carries a token or a username.
fn nothing_secret_in(events: &Mutex<Vec<BTreeMap<String, String>>>) {
    for event in events.lock().unwrap().iter() {
        for (name, value) in event {
            assert!(
                !value.contains(TOKEN) && !value.contains(SECOND_TOKEN),
                "{name} carried a token: {value}"
            );
            assert!(
                !value.contains("registry-robot"),
                "{name} carried a username: {value}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Registering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn each_kind_is_registered_where_its_kind_says_and_listed_as_proven() {
    let harness = harness(None);
    let plane = &harness.plane;

    let (status, _, ghcr) = register(plane, json!({"kind": "ghcr"})).await;
    assert_eq!(status, StatusCode::CREATED, "{ghcr}");
    assert_eq!(ghcr["host"], "ghcr.io");
    assert_eq!(ghcr["endpoint"], "https://ghcr.io");
    assert_eq!(ghcr["credential"], Value::Null);

    let (status, _, hub) = register(plane, json!({"kind": "dockerHub"})).await;
    assert_eq!(status, StatusCode::CREATED, "{hub}");
    assert_eq!(hub["host"], "docker.io");
    assert_eq!(hub["endpoint"], "https://registry-1.docker.io");

    harness
        .connector
        .script(|script| script.realm = Some("https://auth.example.com".to_owned()));
    let (status, _, own) = register(
        plane,
        json!({"kind": "distribution", "endpoint": "https://registry.example.com:5000/"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{own}");
    assert_eq!(own["host"], "registry.example.com:5000");
    assert_eq!(own["endpoint"], "https://registry.example.com:5000");
    assert_eq!(own["realmOrigin"], "https://auth.example.com");
    assert_eq!(own["registeredBy"], support::OPERATOR);
    assert_eq!(own["installed"], true);

    let (status, _, listing) = call(plane, "GET", "/api/integrations/registries", None).await;
    assert_eq!(status, StatusCode::OK);
    let hosts: Vec<&str> = listing["registries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|registry| registry["host"].as_str().unwrap())
        .collect();
    assert_eq!(hosts, ["ghcr.io", "docker.io", "registry.example.com:5000"]);
    assert_eq!(listing["deployment"], Value::Null);
    assert_eq!(
        harness.connector.last_installed(),
        ["docker.io", "ghcr.io", "registry.example.com:5000"]
    );
}

#[tokio::test]
async fn a_registration_the_rules_refuse_is_refused_naming_the_rule() {
    let harness = harness(None);
    let plane = &harness.plane;

    for (body, expected_code, rule) in [
        (
            json!({"kind": "distribution", "endpoint": "http://registry.example.com"}),
            "registry_invalid",
            "HTTPS",
        ),
        (
            json!({"kind": "distribution", "endpoint": "https://10.0.0.8"}),
            "registry_invalid",
            "IP address",
        ),
        (
            json!({"kind": "distribution", "endpoint": "https://registry.example.com/v2/"}),
            "registry_invalid",
            "path",
        ),
        (json!({"kind": "distribution"}), "registry_invalid", "endpoint"),
        (
            json!({"kind": "ghcr", "endpoint": "https://ghcr.example.com"}),
            "registry_invalid",
            "fixed by its kind",
        ),
        (
            json!({"kind": "distribution", "endpoint": "https://ghcr.io"}),
            "registry_invalid",
            "ghcr",
        ),
        (
            json!({"kind": "ghcr", "username": "registry-robot"}),
            "registry_invalid",
            "together",
        ),
        (
            json!({"kind": "ghcr", "host": "ghcr.io"}),
            "invalid_request",
            "host",
        ),
        (json!({"kind": "quay"}), "invalid_request", "quay"),
    ] {
        let (status, _, refused) = register(plane, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {refused}");
        assert_eq!(code(&refused), expected_code, "{body}: {refused}");
        let message = refused["error"]["message"].as_str().unwrap();
        assert!(message.contains(rule), "{body}: {message}");
    }
    assert!(
        harness.connector.built.lock().unwrap().is_empty(),
        "nothing was asked of a registry"
    );
}

#[tokio::test]
async fn a_second_registration_for_a_host_is_a_conflict() {
    let harness = harness(None);
    register(&harness.plane, json!({"kind": "ghcr"})).await;

    let (status, _, refused) = register(&harness.plane, json!({"kind": "ghcr"})).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(code(&refused), "registry_exists");
}

#[tokio::test]
async fn the_deployments_host_is_registered_only_at_the_deployments_endpoint() {
    let harness = harness(Some(DeploymentRegistry::new(
        "ghcr.io",
        "https://ghcr-mirror.example.com/",
    )));

    let (status, _, refused) = register(&harness.plane, json!({"kind": "ghcr"})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(code(&refused), "registry_endpoint_differs");

    let (_, _, listing) = call(&harness.plane, "GET", "/api/integrations/registries", None).await;
    assert_eq!(
        listing["deployment"],
        json!({"host": "ghcr.io", "endpoint": "https://ghcr-mirror.example.com"})
    );
}

#[tokio::test]
async fn a_registration_for_the_deployments_host_at_its_endpoint_is_listed_as_the_deployments() {
    let harness = harness(Some(DeploymentRegistry::new("ghcr.io", "https://ghcr.io")));

    let (status, _, registered) = register(&harness.plane, json!({"kind": "ghcr"})).await;

    assert_eq!(status, StatusCode::CREATED, "{registered}");
    assert_eq!(registered["deployment"], true);
}

#[tokio::test]
async fn a_proof_that_fails_answers_its_own_code_and_records_nothing() {
    for (error, status, expected_code, retry) in [
        (
            RegistryError::Denied {
                detail: "proving the registry: the realm refused this registry's credential".to_owned(),
            },
            StatusCode::BAD_GATEWAY,
            "registry_refused",
            false,
        ),
        (
            RegistryError::Refused {
                detail: "proving the registry was refused with 404".to_owned(),
            },
            StatusCode::UNPROCESSABLE_ENTITY,
            "registry_not_proven",
            false,
        ),
        (
            RegistryError::Unavailable {
                detail: "proving the registry timed out".to_owned(),
            },
            StatusCode::SERVICE_UNAVAILABLE,
            "registry_unavailable",
            true,
        ),
    ] {
        let harness = harness(None);
        harness.connector.script(|script| script.prove = Some(error));

        let (got, retry_after, refused) = register(
            &harness.plane,
            json!({"kind": "ghcr", "username": "registry-robot", "token": TOKEN}),
        )
        .await;

        assert_eq!(got, status, "{refused}");
        assert_eq!(code(&refused), expected_code);
        assert_eq!(retry_after, retry, "{expected_code}");
        assert!(harness.store.load().await.unwrap().is_empty());
    }
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_credential_is_set_replaced_and_removed_and_never_shown_or_logged() {
    let events = capture();
    let harness = harness(None);
    let plane = &harness.plane;

    let (status, _, registered) = register(
        plane,
        json!({"kind": "ghcr", "username": "registry-robot", "token": TOKEN}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{registered}");
    assert_eq!(registered["credential"]["username"], "registry-robot");
    assert_eq!(registered["credential"]["setBy"], support::OPERATOR);
    assert_eq!(registered["credential"]["refused"], false);
    assert!(harness.connector.last_built().credential);

    let (status, _, replaced) = call(
        plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/credential",
        Some(json!({"username": "registry-robot", "token": SECOND_TOKEN})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replaced}");
    assert_eq!(replaced["credential"]["username"], "registry-robot");

    let (status, _, removed) = call(
        plane,
        "DELETE",
        "/api/integrations/registries/ghcr.io/credential",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["credential"], Value::Null);
    assert!(!harness.connector.last_built().credential);

    let (_, _, listing) = call(plane, "GET", "/api/integrations/registries", None).await;
    assert_eq!(listing["registries"][0]["credential"], Value::Null);

    SINK.with(|sink| *sink.borrow_mut() = None);
    nothing_secret_in(&events);
    let operations: Vec<(String, String)> = audited(&events)
        .into_iter()
        .map(|(operation, outcome, _)| (operation, outcome))
        .collect();
    assert_eq!(
        operations,
        [
            ("register".to_owned(), "succeeded".to_owned()),
            ("set_credential".to_owned(), "succeeded".to_owned()),
            ("remove_credential".to_owned(), "succeeded".to_owned()),
        ]
    );
}

#[tokio::test]
async fn a_credential_its_realm_refused_is_shown_as_refused() {
    let harness = harness(None);
    harness.connector.script(|script| script.refused = true);

    let (_, _, registered) = register(
        &harness.plane,
        json!({"kind": "ghcr", "username": "registry-robot", "token": TOKEN}),
    )
    .await;

    assert_eq!(registered["credential"]["refused"], true);
}

#[tokio::test]
async fn a_credential_for_a_registry_nobody_registered_is_not_found() {
    let harness = harness(None);

    let (status, _, refused) = call(
        &harness.plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/credential",
        Some(json!({"username": "registry-robot", "token": TOKEN})),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(code(&refused), "registry_not_found");
}

#[tokio::test]
async fn a_credential_its_realm_refuses_while_a_repository_is_proven_is_marked_where_discovery_reads() {
    let harness = harness(None);
    let plane = &harness.plane;
    harness.connector.script(|script| {
        script.denied.insert("ghcr.io/fieldstatenz/gone".to_owned());
    });
    register(
        plane,
        json!({"kind": "ghcr", "username": "registry-robot", "token": TOKEN}),
    )
    .await;

    let (status, retry_after, refused) = call(
        plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/repositories/entry/fieldstatenz/gone",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{refused}");
    assert!(
        !retry_after,
        "a refused credential is not advertised as transient"
    );
    assert_eq!(code(&refused), "registry_refused");

    let (_, _, listed) = call(plane, "GET", "/api/integrations/registries", None).await;
    assert_eq!(listed["registries"][0]["credential"]["refused"], true, "{listed}");
}

// ---------------------------------------------------------------------------
// Repositories and versions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_repository_is_registered_once_it_is_readable_and_its_versions_listed() {
    let harness = harness(None);
    let plane = &harness.plane;
    register(
        plane,
        json!({"kind": "ghcr", "username": "registry-robot", "token": TOKEN}),
    )
    .await;

    let (status, _, added) = call(
        plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/repositories/entry/fieldstatenz/saas-fabric",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(
        added["repositories"][0]["repository"],
        "ghcr.io/fieldstatenz/saas-fabric"
    );
    assert_eq!(
        added["repositories"][0]["provenAt"],
        support::FIXED_CLOCK_UNIX_SECONDS
    );
    let built = harness.connector.last_built();
    assert!(built.credential);
    assert_eq!(built.repositories, ["ghcr.io/fieldstatenz/saas-fabric"]);

    harness.connector.script(|script| {
        script.tags = ["0.3.0-preview.9", "latest", "0.3.0-preview.10", "0.2.0"]
            .map(str::to_owned)
            .to_vec();
    });
    // The live client was built before the tags were scripted; register it
    // again so the one read through answers them.
    call(
        plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/repositories/entry/fieldstatenz/saas-fabric",
        None,
    )
    .await;
    let (status, _, versions) = call(
        plane,
        "GET",
        "/api/integrations/registries/ghcr.io/versions/fieldstatenz/saas-fabric",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{versions}");
    assert_eq!(
        versions,
        json!({"tags": ["0.3.0-preview.10", "0.3.0-preview.9", "0.2.0"], "other": 1})
    );

    let (status, _, removed) = call(
        plane,
        "DELETE",
        "/api/integrations/registries/ghcr.io/repositories/entry/fieldstatenz/saas-fabric",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["repositories"], json!([]));
    assert!(harness.connector.last_built().repositories.is_empty());
}

#[tokio::test]
async fn a_repository_that_is_not_readable_or_not_named_by_the_rule_is_refused() {
    let events = capture();
    let harness = harness(None);
    let plane = &harness.plane;
    register(plane, json!({"kind": "dockerHub"})).await;
    harness.connector.script(|script| {
        script.unreadable.insert("docker.io/acme/private".to_owned());
    });

    let (status, _, refused) = call(
        plane,
        "PUT",
        "/api/integrations/registries/docker.io/repositories/entry/acme/private",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(code(&refused), "repository_not_readable");
    assert_eq!(
        refused["error"]["message"],
        "docker.io/acme/private is not readable through this registry"
    );

    let (status, _, refused) = call(
        plane,
        "PUT",
        "/api/integrations/registries/docker.io/repositories/entry/nginx",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(code(&refused), "registry_invalid");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("docker.io/library/nginx"),
        "{refused}"
    );

    let (status, _, refused) = call(
        plane,
        "GET",
        "/api/integrations/registries/docker.io/versions/acme/unregistered",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(code(&refused), "registry_not_found");

    SINK.with(|sink| *sink.borrow_mut() = None);
    let audits = audited(&events);
    assert!(
        audits.contains(&(
            "add_repository".to_owned(),
            "repository_not_readable".to_owned(),
            "docker.io".to_owned()
        )),
        "{audits:?}"
    );
}

#[tokio::test]
async fn a_host_that_is_not_one_is_refused_before_any_record_is_looked_up() {
    let harness = harness(None);

    for path in [
        "/api/integrations/registries/10.0.0.1/credential",
        "/api/integrations/registries/GHCR.IO/credential",
    ] {
        let (status, _, refused) = call(&harness.plane, "DELETE", path, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {refused}");
        assert_eq!(code(&refused), "registry_invalid");
    }
}

// ---------------------------------------------------------------------------
// Removing, auditing, and the operator
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_operation_is_audited_with_its_outcome_including_refusals() {
    let events = capture();
    let harness = harness(None);
    let plane = &harness.plane;

    register(plane, json!({"kind": "ghcr"})).await;
    register(plane, json!({"kind": "ghcr"})).await;
    register(
        plane,
        json!({"kind": "distribution", "endpoint": "https://10.1.1.1"}),
    )
    .await;
    call(
        plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/credential",
        Some(json!({"username": "registry-robot", "token": TOKEN})),
    )
    .await;
    call(
        plane,
        "PUT",
        "/api/integrations/registries/ghcr.io/repositories/entry/acme/app",
        None,
    )
    .await;
    call(
        plane,
        "DELETE",
        "/api/integrations/registries/ghcr.io/repositories/entry/acme/app",
        None,
    )
    .await;
    call(
        plane,
        "DELETE",
        "/api/integrations/registries/ghcr.io/credential",
        None,
    )
    .await;
    let (status, _, _) = call(plane, "DELETE", "/api/integrations/registries/ghcr.io", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, refused) = call(plane, "DELETE", "/api/integrations/registries/ghcr.io", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(code(&refused), "registry_not_found");

    SINK.with(|sink| *sink.borrow_mut() = None);
    nothing_secret_in(&events);
    let expected: Vec<(String, String, String)> = [
        ("register", "succeeded", "ghcr.io"),
        ("register", "registry_exists", "ghcr.io"),
        ("register", "registry_invalid", ""),
        ("set_credential", "succeeded", "ghcr.io"),
        ("add_repository", "succeeded", "ghcr.io"),
        ("remove_repository", "succeeded", "ghcr.io"),
        ("remove_credential", "succeeded", "ghcr.io"),
        ("remove", "succeeded", "ghcr.io"),
        ("remove", "registry_not_found", "ghcr.io"),
    ]
    .iter()
    .map(|(operation, outcome, host)| ((*operation).to_owned(), (*outcome).to_owned(), (*host).to_owned()))
    .collect();
    assert_eq!(audited(&events), expected);

    let operators: BTreeSet<String> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.get("event").map(String::as_str) == Some("control_plane.audit.registry"))
        .map(|event| event["operator"].clone())
        .collect();
    assert_eq!(operators, BTreeSet::from([support::OPERATOR.to_owned()]));
}

#[tokio::test]
async fn a_request_refused_before_the_service_sees_it_is_audited_too() {
    let events = capture();
    let harness = harness(None);
    let plane = &harness.plane;
    register(plane, json!({"kind": "dockerHub"})).await;

    let (status, _, _) = register(plane, json!({"kind": "ghcr", "host": "ghcr.io"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = call(plane, "DELETE", "/api/integrations/registries/10.0.0.1", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = call(
        plane,
        "PUT",
        "/api/integrations/registries/docker.io/repositories/entry/nginx",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = call(
        plane,
        "PUT",
        "/api/integrations/registries/docker.io/credential",
        Some(json!({"username": "robot", "token": TOKEN, "scope": "push"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    SINK.with(|sink| *sink.borrow_mut() = None);
    nothing_secret_in(&events);
    let refusals: Vec<(String, String)> = audited(&events)
        .into_iter()
        .skip(1)
        .map(|(operation, _, host)| (operation, host))
        .collect();
    assert_eq!(
        refusals,
        [
            ("register".to_owned(), String::new()),
            ("remove".to_owned(), String::new()),
            ("add_repository".to_owned(), "docker.io".to_owned()),
            ("set_credential".to_owned(), "docker.io".to_owned()),
        ]
    );
    assert!(
        audited(&events)
            .iter()
            .skip(1)
            .all(|(_, outcome, _)| outcome != "succeeded"),
        "{:?}",
        audited(&events)
    );
}

#[tokio::test]
async fn no_registry_route_answers_without_an_operator() {
    let harness = harness(None);

    for (method, path) in [
        ("GET", "/api/integrations/registries"),
        ("POST", "/api/integrations/registries"),
        ("DELETE", "/api/integrations/registries/ghcr.io"),
        ("PUT", "/api/integrations/registries/ghcr.io/credential"),
        (
            "PUT",
            "/api/integrations/registries/ghcr.io/repositories/entry/acme/app",
        ),
        ("GET", "/api/integrations/registries/ghcr.io/versions/acme/app"),
    ] {
        let request = http::Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json!({"kind": "ghcr"}).to_string()))
            .unwrap();
        let response = send(&harness.plane.router, request).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{method} {path}");
    }
}

// ---------------------------------------------------------------------------
// Restoring at startup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_credential_that_cannot_be_read_at_startup_leaves_its_registry_read_anonymously_and_flagged() {
    let first = harness(None);
    register(
        &first.plane,
        json!({"kind": "ghcr", "username": "registry-robot", "token": TOKEN}),
    )
    .await;
    first.secrets.fail_reads.store(true, Ordering::SeqCst);

    // A restart: a new service over the same stores.
    let connector = Arc::new(FakeConnector::default());
    let service = Arc::new(RegistryService::new(RegistryServiceParts {
        store: Arc::clone(&first.store) as Arc<dyn RegistryStore>,
        secrets: Arc::clone(&first.secrets) as Arc<dyn SecretStore>,
        connector: Arc::clone(&connector) as Arc<dyn RegistryConnector>,
        clock: Arc::new(FixedClock),
        deployment: None,
    }));
    assert!(!service.restore().await, "incomplete, so it is asked again");
    let restarted = control_plane_with_registries(service);

    let built = connector.last_built();
    assert_eq!(built.host, "ghcr.io");
    assert!(
        !built.credential,
        "a credential that could not be read is not presented"
    );
    assert_eq!(connector.last_installed(), ["ghcr.io"]);

    let (status, _, listing) = call(&restarted, "GET", "/api/integrations/registries", None).await;
    assert_eq!(status, StatusCode::OK);
    let credential = &listing["registries"][0]["credential"];
    assert_eq!(credential["username"], "registry-robot");
    assert_eq!(credential["unreadable"], true);
    assert_eq!(listing["registries"][0]["installed"], true);
}

#[tokio::test]
async fn registries_whose_records_cannot_be_read_are_unavailable_not_absent() {
    struct Unreadable;

    #[async_trait]
    impl RegistryStore for Unreadable {
        async fn load(
            &self,
        ) -> Result<Vec<fabric_control_plane::RegistryRecord>, fabric_control_plane::RegistryStoreError>
        {
            Err(fabric_control_plane::RegistryStoreError::Unavailable)
        }

        async fn save(
            &self,
            _records: &[fabric_control_plane::RegistryRecord],
        ) -> Result<(), fabric_control_plane::RegistryStoreError> {
            Err(fabric_control_plane::RegistryStoreError::Unavailable)
        }
    }

    let service = Arc::new(RegistryService::new(RegistryServiceParts {
        store: Arc::new(Unreadable),
        secrets: Arc::new(InMemorySecretStore::new()),
        connector: Arc::new(FakeConnector::default()),
        clock: Arc::new(FixedClock),
        deployment: None,
    }));
    service.restore().await;
    let plane = control_plane_with_registries(service);

    let (status, retry, refused) = call(&plane, "GET", "/api/integrations/registries", None).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(retry, "an unreadable store may answer shortly");
    assert_eq!(code(&refused), "registries_unavailable");
}

#[tokio::test]
async fn the_integrations_path_still_names_no_integration_but_registries() {
    let harness = harness(None);

    for path in ["/api/integrations/anything", "/api/integrations/registry"] {
        let (status, _, body) = call(&harness.plane, "GET", path, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body, Value::Null, "{path} must be nothing at all");
    }
}
