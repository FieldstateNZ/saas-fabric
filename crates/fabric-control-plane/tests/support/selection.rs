//! A registry holding a described component a catalogue can select (ADR
//! 0026 section 7), and a registry service holding which repositories are
//! registered.
//!
//! Every answer is from memory. Each component descriptor is built and
//! rendered by `fabric-component`, the one contract crate, so what the rule
//! reads is what a publisher's renderer would attach. The registry can also
//! be told to fail every read, to hang, or to hold its first read open
//! until a test lets it go, and counts the reads it was asked and the ones
//! abandoned in flight.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;

use fabric_component::{
    ComponentDescriptor, ComponentName, ComponentSpec, ComponentVersion, ConfigurationField, Digest,
    FieldKind, ImageReference, Repository, Role, ARTIFACT_TYPE,
};
use fabric_control_plane::{
    InMemoryRegistryStore, InMemorySecretStore, Readability, RegistryClient, RegistryConnection,
    RegistryConnector, RegistryHost, RegistryRecord, RegistryService, RegistryServiceParts, RegistryStore,
    SecretName, SecretStore, SecretValue,
};
use fabric_platform_management::{
    Attached, AttachedDescriptor, Provenance, Registry, RegistryError, Resolved,
};

use super::FixedClock;

/// The primary image's repository: where the component descriptor is
/// attached, and whose tags are the versions there are.
pub const PRIMARY: &str = "ghcr.io/acme/reports";

/// The other image's repository.
pub const SIBLING: &str = "ghcr.io/acme/reports-web";

/// Both roles, and where each is published; `api` is the primary.
pub const ROLES: [(&str, &str); 2] = [("api", PRIMARY), ("web", SIBLING)];

/// The title every descriptor here gives the component.
pub const TITLE: &str = "Acme Reports";

/// The field every descriptor here declares.
pub const DECLARED_FIELD: &str = "retention";

/// A well-formed `sha256` digest, the same for the same seed.
pub fn digest(seed: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("sha256:{}", format!("{:016x}", hasher.finish()).repeat(4))
}

/// The digest `role`'s image of `version` is published at.
pub fn image_digest(version: &str, role: &str) -> String {
    digest(&format!("{version}/{role}"))
}

/// The digest of `version`'s component descriptor manifest.
pub fn descriptor_digest(version: &str) -> String {
    digest(&format!("{version}/component descriptor"))
}

/// What `version`'s component descriptor says: both roles at the digests
/// they are published at, and one declared field.
pub fn spec(version: &str) -> ComponentSpec {
    ComponentSpec {
        name: ComponentName::try_new("reports").unwrap(),
        title: TITLE.to_owned(),
        description: "Scheduled reporting over a client's own data.".to_owned(),
        version: ComponentVersion::try_new(version).unwrap(),
        images: ROLES
            .iter()
            .map(|(role, repository)| {
                (
                    Role::try_new(role).unwrap(),
                    ImageReference {
                        repository: Repository::try_new(repository).unwrap(),
                        digest: Digest::try_new(image_digest(version, role)).unwrap(),
                    },
                )
            })
            .collect(),
        capabilities: Vec::new(),
        fields: vec![ConfigurationField {
            key: DECLARED_FIELD.to_owned(),
            label: "Retention".to_owned(),
            kind: FieldKind::Text,
            required: false,
            default: Some("30d".to_owned()),
            options: Vec::new(),
            description: String::new(),
        }],
        resources: Vec::new(),
    }
}

/// `spec`, attached as a publisher attaches it: rendered by
/// `fabric-component`, annotated with `revision` and the version.
pub fn attached(version: &str, spec: ComponentSpec, revision: &str) -> Attached {
    Attached::One(AttachedDescriptor {
        digest: descriptor_digest(version),
        artifact_type: ARTIFACT_TYPE.to_owned(),
        revision: Some(revision.to_owned()),
        version: Some(version.to_owned()),
        document: ComponentDescriptor::new(spec).unwrap().to_json(),
    })
}

/// A registry whose contents a test sets.
#[derive(Default)]
pub struct SelectionRegistry {
    /// `(repository, tag)` to the digest it points at.
    tags: Mutex<BTreeMap<(String, String), String>>,
    /// `(repository, digest)` to the revision its manifest carries.
    manifests: Mutex<BTreeMap<(String, String), Provenance>>,
    /// Primary image digest to what is attached to it.
    attached: Mutex<BTreeMap<String, Attached>>,
    /// What every read fails with, when set.
    failure: Mutex<Option<RegistryError>>,
    /// Whether every read hangs until it is abandoned.
    hangs: AtomicBool,
    /// The hold the next read takes, if a test asked for one: taken by the
    /// first read, so every read after it answers at once.
    hold: Mutex<Option<Hold>>,
    /// How many reads were asked.
    reads: AtomicUsize,
    /// How many reads are in flight now.
    in_flight: Arc<AtomicUsize>,
    /// How many reads were abandoned before they answered.
    abandoned: Arc<AtomicUsize>,
}

impl SelectionRegistry {
    /// `role`'s image of `version`, tagged with it and built from `revision`.
    pub fn publish_image(&self, version: &str, role: &str, revision: &str) {
        let (_, repository) = ROLES.iter().find(|(named, _)| *named == role).unwrap();
        let digest = image_digest(version, role);
        lock(&self.manifests).insert(
            ((*repository).to_owned(), digest.clone()),
            Provenance::Agreed(revision.to_owned()),
        );
        lock(&self.tags).insert(((*repository).to_owned(), version.to_owned()), digest);
    }

    /// Every image of `version`, from `revision`.
    pub fn publish_images(&self, version: &str, revision: &str) {
        for (role, _) in ROLES {
            self.publish_image(version, role, revision);
        }
    }

    /// Attaches `attached` to `version`'s primary image.
    pub fn describe(&self, version: &str, attached: Attached) {
        lock(&self.attached).insert(image_digest(version, "api"), attached);
    }

    /// A whole release from `revision`: both images and the component
    /// descriptor.
    pub fn publish(&self, version: &str, revision: &str) {
        self.publish_images(version, revision);
        self.describe(version, attached(version, spec(version), revision));
    }

    /// Makes every read fail with `error`.
    pub fn fail_with(&self, error: RegistryError) {
        *lock(&self.failure) = Some(error);
    }

    /// Makes every read hang until it is abandoned.
    pub fn hang(&self) {
        self.hangs.store(true, Ordering::SeqCst);
    }

    /// Holds the next read open, and answers it as asked once released.
    ///
    /// A selection's first registry read comes after its catalogue read
    /// and before its write, so a test holding it has a window another
    /// request can land in. Only that one read is held: the reads after it
    /// answer at once, so the resolution completes when it is released.
    pub fn hold_first_read(&self) -> HeldRead {
        let hold = Hold {
            reached: Arc::new(Notify::new()),
            released: Arc::new(Notify::new()),
        };
        let handle = HeldRead {
            reached: Arc::clone(&hold.reached),
            released: Arc::clone(&hold.released),
        };
        *lock(&self.hold) = Some(hold);
        handle
    }

    /// How many reads were asked.
    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    /// How many reads are in flight now.
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// How many reads were abandoned before they answered.
    pub fn abandoned(&self) -> usize {
        self.abandoned.load(Ordering::SeqCst)
    }

    /// Counts a read, and fails or hangs it if told to.
    async fn asked(&self) -> Result<(), RegistryError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = lock(&self.failure).clone() {
            return Err(error);
        }
        if self.hangs.load(Ordering::SeqCst) {
            let _flight = InFlight::start(&self.in_flight, &self.abandoned);
            std::future::pending::<()>().await;
        }
        // Taken out of the lock before waiting: nothing is held across the
        // await, and the second read finds nothing to take.
        let hold = lock(&self.hold).take();
        if let Some(hold) = hold {
            hold.reached.notify_one();
            hold.released.notified().await;
        }
        Ok(())
    }
}

/// A read held open: the registry's side of [`HeldRead`].
struct Hold {
    /// Signalled when the read arrives.
    reached: Arc<Notify>,
    /// Waited on until the test releases it.
    released: Arc<Notify>,
}

/// A test's hold on the registry's next read.
///
/// Each `Notify` keeps one permit, so neither side depends on which of them
/// arrives first: a read that arrives before the test waits is still seen,
/// and a release before the read waits still lets it go.
pub struct HeldRead {
    reached: Arc<Notify>,
    released: Arc<Notify>,
}

impl HeldRead {
    /// How long a held read may take to arrive before the test fails
    /// rather than hangs. A bound, not a schedule: nothing waits this long
    /// unless the read never comes.
    const ARRIVAL_BOUND: Duration = Duration::from_secs(10);

    /// Waits until the held read has arrived and is waiting to be released.
    ///
    /// # Panics
    ///
    /// If no read arrives within [`Self::ARRIVAL_BOUND`].
    pub async fn reached(&self) {
        tokio::time::timeout(Self::ARRIVAL_BOUND, self.reached.notified())
            .await
            .expect("the held registry read must arrive");
    }

    /// Lets the held read answer.
    pub fn release(&self) {
        self.released.notify_one();
    }
}

/// A read in flight, counted abandoned if it is dropped before it answers.
struct InFlight {
    in_flight: Arc<AtomicUsize>,
    abandoned: Arc<AtomicUsize>,
}

impl InFlight {
    fn start(in_flight: &Arc<AtomicUsize>, abandoned: &Arc<AtomicUsize>) -> Self {
        in_flight.fetch_add(1, Ordering::SeqCst);
        Self {
            in_flight: Arc::clone(in_flight),
            abandoned: Arc::clone(abandoned),
        }
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.abandoned.fetch_add(1, Ordering::SeqCst);
    }
}

/// A lock, whether or not a test panicked holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait::async_trait]
impl Registry for SelectionRegistry {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        self.asked().await?;
        let tags: BTreeSet<String> = lock(&self.tags)
            .keys()
            .filter(|(published, _)| published == repository)
            .map(|(_, tag)| tag.clone())
            .collect();
        Ok(tags.into_iter().collect())
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        self.asked().await?;
        let digest = if reference.starts_with("sha256:") {
            reference.to_owned()
        } else {
            let tagged = lock(&self.tags)
                .get(&(repository.to_owned(), reference.to_owned()))
                .cloned();
            let Some(digest) = tagged else {
                return Ok(None);
            };
            digest
        };
        Ok(lock(&self.manifests)
            .get(&(repository.to_owned(), digest.clone()))
            .map(|provenance| Resolved {
                digest,
                provenance: provenance.clone(),
            }))
    }

    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        self.asked().await?;
        if repository != PRIMARY {
            return Ok(Attached::Nothing);
        }
        Ok(lock(&self.attached)
            .get(subject)
            .cloned()
            .unwrap_or(Attached::Nothing))
    }
}

/// How the registry service holds the recorded `ghcr.io` registry now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    /// Restored: a client is installed, presenting any credential.
    Live,

    /// Restored, and its realm has since refused the credential.
    CredentialRefused,

    /// Recorded and not restored yet: no client is installed.
    Recorded,
}

/// A registry service whose store records `ghcr.io` with `repositories`
/// registered under it, and, when `credential` is set, a credential, read
/// through now.
pub async fn registries_holding(repositories: &[&str], credential: bool) -> Arc<RegistryService> {
    registries_held(repositories, credential, Held::Live).await
}

/// As [`registries_holding`], held as `held` says.
///
/// Recorded straight into the store, as a restart would find it: what these
/// tests pin is how a selection is held to what is registered, not how a
/// registration is proven, which `registries.rs` covers.
pub async fn registries_held(repositories: &[&str], credential: bool, held: Held) -> Arc<RegistryService> {
    let record = serde_json::json!({
        "host": "ghcr.io",
        "kind": "ghcr",
        "endpoint": "https://ghcr.io",
        "secretId": SECRET_ID,
        "credential": credential.then(|| serde_json::json!({
            "username": "registry-robot", "setBy": super::OPERATOR, "setAt": 1
        })),
        "registeredBy": super::OPERATOR,
        "registeredAt": 1,
        "repositories": repositories
            .iter()
            .map(|repository| serde_json::json!({"repository": repository, "provenAt": 1}))
            .collect::<Vec<_>>(),
    });
    let records: Vec<RegistryRecord> = serde_json::from_value(serde_json::json!([record])).unwrap();
    let store = Arc::new(InMemoryRegistryStore::new());
    store.save(&records).await.unwrap();
    let secrets = Arc::new(InMemorySecretStore::new());
    let name = SecretName::new(format!("integrations/registries/{SECRET_ID}/credential"));
    secrets
        .put(&name, &SecretValue::new("registry-token"))
        .await
        .unwrap();

    let service = Arc::new(RegistryService::new(RegistryServiceParts {
        store,
        secrets,
        connector: Arc::new(Idle {
            refused: held == Held::CredentialRefused,
        }),
        clock: Arc::new(FixedClock),
        deployment: None,
    }));
    if held != Held::Recorded {
        assert!(service.restore().await, "the harness's records restore");
    }
    service
}

/// A registry service over `store`, not restored: for a store a test makes
/// misbehave.
pub fn registries_over(store: Arc<dyn RegistryStore>) -> Arc<RegistryService> {
    Arc::new(RegistryService::new(RegistryServiceParts {
        store,
        secrets: Arc::new(InMemorySecretStore::new()),
        connector: Arc::new(Idle { refused: false }),
        clock: Arc::new(FixedClock),
        deployment: None,
    }))
}

/// The id the recorded registry's credential is kept under.
const SECRET_ID: &str = "0123456789abcdef";

/// Builds clients that are never asked anything: a selection reads through
/// the registry port, never through a client this service builds. Only
/// whether one is installed, and whether its credential was refused,
/// matters here.
struct Idle {
    refused: bool,
}

impl RegistryConnector for Idle {
    fn connect(&self, _connection: RegistryConnection) -> Result<Arc<dyn RegistryClient>, String> {
        Ok(Arc::new(IdleClient {
            refused: self.refused,
        }))
    }

    fn install(&self, _clients: BTreeMap<RegistryHost, Arc<dyn RegistryClient>>) {}
}

/// A client that answers nothing it is asked.
struct IdleClient {
    refused: bool,
}

#[async_trait::async_trait]
impl RegistryClient for IdleClient {
    async fn prove(&self) -> Result<Option<String>, RegistryError> {
        Err(idle())
    }

    async fn prove_repository(&self, _repository: &Repository) -> Result<Readability<()>, RegistryError> {
        Err(idle())
    }

    async fn version_tags(
        &self,
        _repository: &Repository,
    ) -> Result<Readability<Vec<String>>, RegistryError> {
        Err(idle())
    }

    fn credential_refused(&self) -> bool {
        self.refused
    }
}

/// What an idle client answers.
fn idle() -> RegistryError {
    RegistryError::Unavailable {
        detail: "this harness's client is never asked".to_owned(),
    }
}
