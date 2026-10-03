//! The order a registry change is written in, what is never written when a
//! proof or a write fails, and how a credential's refusal is kept.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use fabric_component::Repository;
use fabric_platform_management::RegistryError;

use crate::fixtures::FixedClock;
use crate::git_integration::{InMemorySecretStore, SecretName, SecretStore, SecretStoreError, SecretValue};
use crate::operator::{Operator, OperatorToken};
use crate::registries::{
    DeploymentRegistry, InMemoryRegistryStore, Readability, RegistryClient, RegistryConnection,
    RegistryConnector, RegistryFailure, RegistryHost, RegistryKind, RegistryRecord, RegistryService,
    RegistryServiceParts, RegistryStore, RegistryStoreError,
};

use super::{CredentialState, Registration};

/// Every write and install, in the order it happened.
type Log = Arc<Mutex<Vec<String>>>;

fn push(log: &Log, entry: impl Into<String>) {
    log.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry.into());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

fn on(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

/// A secret store that logs every write, and fails as a test switches it to.
#[derive(Default)]
struct Secrets {
    held: InMemorySecretStore,
    log: Log,
    fail_puts: AtomicBool,
    fail_gets: AtomicBool,
    fail_deletes: AtomicBool,
}

#[async_trait]
impl SecretStore for Secrets {
    async fn get(&self, name: &SecretName) -> Result<Option<SecretValue>, SecretStoreError> {
        if on(&self.fail_gets) {
            return Err(SecretStoreError::Unavailable);
        }
        self.held.get(name).await
    }

    async fn put(&self, name: &SecretName, value: &SecretValue) -> Result<(), SecretStoreError> {
        if on(&self.fail_puts) {
            return Err(SecretStoreError::Unavailable);
        }
        push(&self.log, format!("put {name}"));
        self.held.put(name, value).await
    }

    async fn delete(&self, name: &SecretName) -> Result<(), SecretStoreError> {
        if on(&self.fail_deletes) {
            return Err(SecretStoreError::Unavailable);
        }
        push(&self.log, format!("delete {name}"));
        self.held.delete(name).await
    }

    fn describe(&self) -> String {
        "a logging secret store".to_owned()
    }
}

/// A record store that logs every save, and can fail or hold garbage.
#[derive(Default)]
struct Records {
    held: InMemoryRegistryStore,
    log: Log,
    fail_saves: AtomicBool,
    malformed: bool,
}

#[async_trait]
impl RegistryStore for Records {
    async fn load(&self) -> Result<Vec<RegistryRecord>, RegistryStoreError> {
        if self.malformed {
            return Err(RegistryStoreError::Malformed);
        }
        self.held.load().await
    }

    async fn save(&self, records: &[RegistryRecord]) -> Result<(), RegistryStoreError> {
        if on(&self.fail_saves) {
            return Err(RegistryStoreError::Unavailable);
        }
        let hosts: Vec<&str> = records.iter().map(|record| record.host.as_str()).collect();
        push(&self.log, format!("save [{}]", hosts.join(",")));
        self.held.save(records).await
    }
}

/// A client that answers as it was scripted, when it presents a
/// credential: `unreadable` repositories are not readable, and `denied`
/// ones have the realm refuse the credential — marking it, after which
/// nothing that presents it is asked.
struct Client {
    prove: Result<Option<String>, RegistryError>,
    unreadable: BTreeSet<String>,
    denied: BTreeSet<String>,
    mark: Option<Arc<AtomicBool>>,
}

impl Client {
    fn denied(&self) -> RegistryError {
        if let Some(mark) = &self.mark {
            mark.store(true, Ordering::SeqCst);
        }
        RegistryError::Denied {
            detail: "proving a repository: the realm refused this registry's credential".to_owned(),
        }
    }
}

#[async_trait]
impl RegistryClient for Client {
    async fn prove(&self) -> Result<Option<String>, RegistryError> {
        self.prove.clone()
    }

    async fn prove_repository(&self, repository: &Repository) -> Result<Readability<()>, RegistryError> {
        if self.mark.is_none() {
            return Ok(Readability::Readable(()));
        }
        if self.credential_refused() || self.denied.contains(repository.as_str()) {
            return Err(self.denied());
        }
        if self.unreadable.contains(repository.as_str()) {
            return Ok(Readability::NotReadable);
        }
        Ok(Readability::Readable(()))
    }

    async fn version_tags(
        &self,
        _repository: &Repository,
    ) -> Result<Readability<Vec<String>>, RegistryError> {
        Ok(Readability::Readable(vec!["1.0.0".to_owned()]))
    }

    fn credential_refused(&self) -> bool {
        self.mark.as_deref().is_some_and(on)
    }
}

/// A connector whose clients prove as scripted, and which logs installs.
struct Connector {
    log: Log,
    prove: Result<Option<String>, RegistryError>,
    unreadable: BTreeSet<String>,
    denied: BTreeSet<String>,
    connections: Mutex<Vec<RegistryConnection>>,
}

impl RegistryConnector for Connector {
    fn connect(&self, connection: RegistryConnection) -> Result<Arc<dyn RegistryClient>, String> {
        let mark = connection
            .credential
            .as_ref()
            .map(crate::registries::RegistryCredential::refusal_mark);
        self.connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(connection);
        Ok(Arc::new(Client {
            prove: self.prove.clone(),
            unreadable: self.unreadable.clone(),
            denied: self.denied.clone(),
            mark,
        }))
    }

    fn install(&self, clients: BTreeMap<RegistryHost, Arc<dyn RegistryClient>>) {
        let hosts: Vec<&str> = clients.keys().map(RegistryHost::as_str).collect();
        push(&self.log, format!("install [{}]", hosts.join(",")));
    }
}

/// What a test can change about the world.
#[derive(Default)]
struct World {
    fail_puts: bool,
    fail_saves: bool,
    malformed: bool,
    refuse_proof: Option<RegistryError>,
    unreadable: Vec<&'static str>,
    denied: Vec<&'static str>,
    deployment: Option<DeploymentRegistry>,
}

struct Harness {
    service: RegistryService,
    log: Log,
    connector: Arc<Connector>,
    records: Arc<Records>,
    secrets: Arc<Secrets>,
}

fn connector(world: &World, log: &Log) -> Arc<Connector> {
    Arc::new(Connector {
        log: Arc::clone(log),
        prove: world
            .refuse_proof
            .clone()
            .map_or(Ok(Some("https://auth.example.com".to_owned())), Err),
        unreadable: world.unreadable.iter().map(|name| (*name).to_owned()).collect(),
        denied: world.denied.iter().map(|name| (*name).to_owned()).collect(),
        connections: Mutex::default(),
    })
}

fn harness(world: World) -> Harness {
    let log: Log = Arc::default();
    let records = Arc::new(Records {
        log: Arc::clone(&log),
        fail_saves: AtomicBool::new(world.fail_saves),
        malformed: world.malformed,
        ..Records::default()
    });
    let secrets = Arc::new(Secrets {
        log: Arc::clone(&log),
        fail_puts: AtomicBool::new(world.fail_puts),
        ..Secrets::default()
    });
    let connector = connector(&world, &log);
    let service = service(&records, &secrets, &connector, world.deployment);
    Harness {
        service,
        log,
        connector,
        records,
        secrets,
    }
}

fn service(
    records: &Arc<Records>,
    secrets: &Arc<Secrets>,
    connector: &Arc<Connector>,
    deployment: Option<DeploymentRegistry>,
) -> RegistryService {
    RegistryService::new(RegistryServiceParts {
        store: Arc::clone(records) as Arc<dyn RegistryStore>,
        secrets: Arc::clone(secrets) as Arc<dyn SecretStore>,
        connector: Arc::clone(connector) as Arc<dyn RegistryConnector>,
        clock: Arc::new(FixedClock),
        deployment,
    })
}

impl Harness {
    /// The same stores, read by a service started afresh: a restart.
    fn restarted(&self, deployment: Option<DeploymentRegistry>) -> RegistryService {
        service(&self.records, &self.secrets, &self.connector, deployment)
    }

    fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
}

fn operator() -> Operator {
    Operator::new("brett@example.com", OperatorToken::new("a-fixture-bearer"))
}

fn ghcr(credential: bool) -> Registration {
    Registration {
        kind: RegistryKind::Ghcr,
        endpoint: None,
        username: credential.then(|| "brett".to_owned()),
        token: credential.then(|| SecretValue::new("ghp_a-token-that-must-not-leak")),
    }
}

fn host(text: &str) -> RegistryHost {
    RegistryHost::parse(text).unwrap()
}

fn repository(text: &str) -> Repository {
    Repository::try_new(text).unwrap()
}

#[tokio::test]
async fn a_registration_writes_its_credential_before_its_record_and_installs_last() {
    let plane = harness(World::default());

    plane.service.register(&operator(), ghcr(true)).await.unwrap();

    let log = entries(&plane.log);
    assert_eq!(log.len(), 3, "{log:?}");
    assert!(log[0].starts_with("put integrations/registries/"), "{log:?}");
    assert!(log[0].ends_with("/credential"), "{log:?}");
    assert_eq!(log[1], "save [ghcr.io]");
    assert_eq!(log[2], "install [ghcr.io]");
}

#[tokio::test]
async fn a_registration_that_does_not_prove_writes_nothing() {
    let plane = harness(World {
        refuse_proof: Some(RegistryError::Denied {
            detail: "the realm refused this registry's credential".to_owned(),
        }),
        ..World::default()
    });

    let failure = plane.service.register(&operator(), ghcr(true)).await.unwrap_err();

    assert!(matches!(failure, RegistryFailure::Refused(_)), "{failure:?}");
    assert!(entries(&plane.log).is_empty(), "{:?}", entries(&plane.log));
}

#[tokio::test]
async fn a_record_that_cannot_be_saved_takes_its_new_credential_with_it() {
    let plane = harness(World {
        fail_saves: true,
        ..World::default()
    });

    let failure = plane.service.register(&operator(), ghcr(true)).await.unwrap_err();

    assert_eq!(failure, RegistryFailure::StoreUnavailable);
    let log = entries(&plane.log);
    assert!(log[0].starts_with("put "), "{log:?}");
    assert_eq!(log[1], log[0].replacen("put ", "delete ", 1), "{log:?}");
    assert_eq!(
        log.len(),
        2,
        "nothing is installed for a registry that was not recorded: {log:?}"
    );
}

#[tokio::test]
async fn a_credential_that_cannot_be_written_leaves_nothing_recorded() {
    let plane = harness(World {
        fail_puts: true,
        ..World::default()
    });

    let failure = plane.service.register(&operator(), ghcr(true)).await.unwrap_err();

    assert_eq!(failure, RegistryFailure::StoreUnavailable);
    assert!(entries(&plane.log).is_empty());
}

#[tokio::test]
async fn a_replaced_credential_is_written_beside_the_old_one_and_the_old_one_forgotten_last() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    let first = entries(&plane.log)[0].replacen("put ", "", 1);
    plane.log.lock().unwrap().clear();

    plane
        .service
        .set_credential(
            &operator(),
            host("ghcr.io"),
            "brett".to_owned(),
            SecretValue::new("ghp_second"),
        )
        .await
        .unwrap();

    let log = entries(&plane.log);
    assert_eq!(log.len(), 4, "{log:?}");
    assert!(
        log[0].starts_with("put ") && log[0] != format!("put {first}"),
        "{log:?}"
    );
    assert_eq!(log[1], "save [ghcr.io]");
    assert_eq!(log[2], "install [ghcr.io]");
    assert_eq!(log[3], format!("delete {first}"));
}

#[tokio::test]
async fn a_new_credential_is_proven_against_every_registered_repository_before_anything_is_written() {
    // Readable anonymously, and not with the credential being set: the
    // credential would make a registered repository unreadable, so it is
    // refused before a byte is written.
    let plane = harness(World {
        unreadable: vec!["ghcr.io/acme/app"],
        ..World::default()
    });
    plane.service.register(&operator(), ghcr(false)).await.unwrap();
    plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/app"))
        .await
        .unwrap();
    plane.log.lock().unwrap().clear();

    let failure = plane
        .service
        .set_credential(
            &operator(),
            host("ghcr.io"),
            "brett".to_owned(),
            SecretValue::new("ghp_x"),
        )
        .await
        .unwrap_err();

    assert_eq!(
        failure,
        RegistryFailure::RepositoryNotReadable {
            repository: "ghcr.io/acme/app".to_owned()
        }
    );
    assert!(entries(&plane.log).is_empty(), "{:?}", entries(&plane.log));
}

#[tokio::test]
async fn removing_a_registry_stops_reading_through_it_then_forgets_its_record_then_its_credential() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    let credential = entries(&plane.log)[0].replacen("put ", "", 1);
    plane.log.lock().unwrap().clear();

    plane.service.remove(&operator(), host("ghcr.io")).await.unwrap();

    assert_eq!(
        entries(&plane.log),
        [
            "install []".to_owned(),
            "save []".to_owned(),
            format!("delete {credential}"),
        ]
    );
}

#[tokio::test]
async fn removing_a_credential_reads_anonymously_and_records_it_before_the_credential_is_deleted() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    let credential = entries(&plane.log)[0].replacen("put ", "", 1);
    plane.log.lock().unwrap().clear();

    let listed = plane
        .service
        .remove_credential(&operator(), host("ghcr.io"))
        .await
        .unwrap();

    assert!(listed.record.credential.is_none());
    assert_eq!(
        entries(&plane.log),
        [
            "install [ghcr.io]".to_owned(),
            "save [ghcr.io]".to_owned(),
            format!("delete {credential}"),
        ]
    );
    let last = plane
        .connector
        .connections
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert!(last.credential.is_none(), "the installed client presents nothing");
}

#[tokio::test]
async fn a_repository_on_another_host_is_refused_before_anything_is_asked() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(false)).await.unwrap();
    plane.log.lock().unwrap().clear();

    let failure = plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("quay.io/acme/app"))
        .await
        .unwrap_err();

    assert!(matches!(failure, RegistryFailure::Invalid(_)), "{failure:?}");
    assert!(entries(&plane.log).is_empty());
}

#[tokio::test]
async fn a_credential_is_presented_only_for_the_repositories_registered_under_it() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/app"))
        .await
        .unwrap();

    let last = plane
        .connector
        .connections
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert!(last.credential.is_some());
    assert_eq!(last.repositories, [repository("ghcr.io/acme/app")]);
}

#[tokio::test]
async fn a_record_set_that_will_not_parse_is_never_saved_over() {
    let plane = harness(World {
        malformed: true,
        ..World::default()
    });

    let failure = plane
        .service
        .register(&operator(), ghcr(false))
        .await
        .unwrap_err();

    assert_eq!(failure, RegistryFailure::StoreInvalid);
    assert!(entries(&plane.log).is_empty());
}

#[tokio::test]
async fn the_deployments_host_is_registered_only_at_the_deployments_endpoint() {
    let plane = harness(World {
        deployment: Some(DeploymentRegistry::new("ghcr.io", "https://mirror.example.com")),
        ..World::default()
    });

    let failure = plane.service.register(&operator(), ghcr(true)).await.unwrap_err();

    assert_eq!(
        failure,
        RegistryFailure::EndpointDiffers {
            host: "ghcr.io".to_owned()
        }
    );
    assert!(entries(&plane.log).is_empty());
}

#[tokio::test]
async fn a_second_registration_for_one_host_is_refused() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(false)).await.unwrap();

    let failure = plane
        .service
        .register(&operator(), ghcr(false))
        .await
        .unwrap_err();

    assert_eq!(
        failure,
        RegistryFailure::Exists {
            host: "ghcr.io".to_owned()
        }
    );
}

#[tokio::test]
async fn the_in_memory_store_holds_nothing_until_it_is_saved_and_a_save_replaces_the_set() {
    let store = InMemoryRegistryStore::new();
    assert!(store.load().await.unwrap().is_empty());

    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(false)).await.unwrap();
    let (listed, _) = plane.service.list().await.unwrap();
    let record = listed[0].record.clone();

    store.save(&[record.clone(), record.clone()]).await.unwrap();
    assert_eq!(store.load().await.unwrap().len(), 2);
    store.save(&[record]).await.unwrap();
    assert_eq!(store.load().await.unwrap().len(), 1);
    store.save(&[]).await.unwrap();
    assert!(store.load().await.unwrap().is_empty());
}

fn credential_of(listed: &super::Listed) -> CredentialState {
    listed.credential
}

async fn registered_with(plane: &Harness, repositories: &[&str]) {
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    for name in repositories {
        plane
            .service
            .add_repository(&operator(), host("ghcr.io"), repository(name))
            .await
            .unwrap();
    }
}

async fn listed(service: &RegistryService) -> super::Listed {
    let (mut listed, _) = service.list().await.unwrap();
    listed.remove(0)
}

#[tokio::test]
async fn a_credential_refused_while_adding_a_repository_marks_the_client_discovery_reads_through() {
    let plane = harness(World {
        denied: vec!["ghcr.io/acme/gone"],
        ..World::default()
    });
    registered_with(&plane, &["ghcr.io/acme/app"]).await;

    let failure = plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/gone"))
        .await
        .unwrap_err();

    assert!(matches!(failure, RegistryFailure::Refused(_)), "{failure:?}");
    assert_eq!(
        credential_of(&listed(&plane.service).await),
        CredentialState::Refused
    );
}

#[tokio::test]
async fn removing_a_repository_never_rearms_a_refused_credential() {
    let plane = harness(World {
        denied: vec!["ghcr.io/acme/gone"],
        ..World::default()
    });
    registered_with(&plane, &["ghcr.io/acme/app", "ghcr.io/acme/other"]).await;
    let _ = plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/gone"))
        .await;

    plane
        .service
        .remove_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/other"))
        .await
        .unwrap();

    assert_eq!(
        credential_of(&listed(&plane.service).await),
        CredentialState::Refused
    );
    let failure = plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/other"))
        .await
        .unwrap_err();
    assert!(
        matches!(failure, RegistryFailure::Refused(_)),
        "not presented again until it is replaced: {failure:?}"
    );
}

#[tokio::test]
async fn a_replacement_its_realm_refuses_leaves_the_held_credential_unmarked() {
    let plane = harness(World {
        denied: vec!["ghcr.io/acme/app"],
        ..World::default()
    });
    plane.service.register(&operator(), ghcr(false)).await.unwrap();
    plane
        .service
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/app"))
        .await
        .unwrap();

    let failure = plane
        .service
        .set_credential(
            &operator(),
            host("ghcr.io"),
            "brett".to_owned(),
            SecretValue::new("ghp_x"),
        )
        .await
        .unwrap_err();

    assert!(matches!(failure, RegistryFailure::Refused(_)), "{failure:?}");
    assert_eq!(
        credential_of(&listed(&plane.service).await),
        CredentialState::Presented
    );
}

#[tokio::test]
async fn a_credential_whose_deletion_fails_after_its_removal_is_recorded_stays_withdrawn() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    plane.secrets.fail_deletes.store(true, Ordering::SeqCst);
    plane.clear();

    let removed = plane
        .service
        .remove_credential(&operator(), host("ghcr.io"))
        .await
        .unwrap();

    assert!(removed.record.credential.is_none());
    assert_eq!(entries(&plane.log), ["install [ghcr.io]", "save [ghcr.io]"]);
    let last = plane
        .connector
        .connections
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert!(last.credential.is_none(), "the installed client presents nothing");
}

#[tokio::test]
async fn a_removal_whose_record_cannot_be_saved_deletes_nothing_and_reads_as_before() {
    let plane = harness(World::default());
    plane.service.register(&operator(), ghcr(true)).await.unwrap();
    plane.records.fail_saves.store(true, Ordering::SeqCst);
    plane.clear();

    let failure = plane
        .service
        .remove(&operator(), host("ghcr.io"))
        .await
        .unwrap_err();

    assert_eq!(failure, RegistryFailure::StoreUnavailable);
    assert_eq!(entries(&plane.log), ["install []", "install [ghcr.io]"]);
}

#[tokio::test]
async fn a_credential_the_store_no_longer_holds_is_refused_as_not_retryable() {
    let plane = harness(World::default());
    registered_with(&plane, &[]).await;
    let (listed_now, _) = plane.service.list().await.unwrap();
    plane
        .secrets
        .held
        .delete(&listed_now[0].record.secret_id.credential())
        .await
        .unwrap();

    let restarted = plane.restarted(None);
    assert!(restarted.restore().await, "a missing secret is not waited for");
    assert_eq!(
        credential_of(&listed(&restarted).await),
        CredentialState::Unreadable
    );

    let failure = restarted
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/app"))
        .await
        .unwrap_err();
    assert_eq!(
        failure,
        RegistryFailure::CredentialUnreadable {
            host: "ghcr.io".to_owned()
        }
    );
}

#[tokio::test]
async fn a_restore_the_secret_store_did_not_answer_is_incomplete_until_it_does() {
    let plane = harness(World::default());
    registered_with(&plane, &[]).await;
    plane.secrets.fail_gets.store(true, Ordering::SeqCst);

    let restarted = plane.restarted(None);
    assert!(!restarted.restore().await);
    assert_eq!(
        credential_of(&listed(&restarted).await),
        CredentialState::Unreadable
    );

    plane.secrets.fail_gets.store(false, Ordering::SeqCst);
    assert!(restarted.restore().await);
    assert_eq!(
        credential_of(&listed(&restarted).await),
        CredentialState::Presented
    );
    let last = plane
        .connector
        .connections
        .lock()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert!(last.credential.is_some(), "restored with its credential");
}

#[tokio::test]
async fn a_deployment_host_record_at_another_endpoint_is_never_read_with_its_credential() {
    let plane = harness(World {
        deployment: Some(DeploymentRegistry::new("ghcr.io", "https://ghcr.io")),
        ..World::default()
    });
    registered_with(&plane, &["ghcr.io/acme/app"]).await;
    let moved = plane.restarted(Some(DeploymentRegistry::new(
        "ghcr.io",
        "https://mirror.example.com",
    )));
    moved.restore().await;
    let differs = RegistryFailure::EndpointDiffers {
        host: "ghcr.io".to_owned(),
    };
    plane.connector.connections.lock().unwrap().clear();

    let adding = moved
        .add_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/new"))
        .await;
    let removing = moved
        .remove_repository(&operator(), host("ghcr.io"), repository("ghcr.io/acme/app"))
        .await;
    let replacing = moved
        .set_credential(
            &operator(),
            host("ghcr.io"),
            "brett".to_owned(),
            SecretValue::new("ghp_y"),
        )
        .await;
    let listing = moved
        .versions(&host("ghcr.io"), &repository("ghcr.io/acme/app"))
        .await;

    assert_eq!(adding.unwrap_err(), differs);
    assert_eq!(removing.unwrap_err(), differs);
    assert_eq!(replacing.unwrap_err(), differs);
    assert_eq!(listing.unwrap_err(), differs);
    assert!(
        plane.connector.connections.lock().unwrap().is_empty(),
        "no client was built at the deployment's new endpoint"
    );

    let withdrawn = moved
        .remove_credential(&operator(), host("ghcr.io"))
        .await
        .unwrap();
    assert!(!withdrawn.installed, "withdrawing it installs nothing either");
    assert!(plane.connector.connections.lock().unwrap().is_empty());
}
