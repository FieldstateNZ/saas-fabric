//! A registry a described-component test arranges by hand: tags, manifests
//! by digest, what is attached to each, and which reads fail.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, PoisonError};

use fabric_component::{
    ComponentDescriptor, ComponentName, ComponentSpec, ComponentVersion, Digest, ImageReference, Repository,
    Role, ARTIFACT_TYPE,
};

use crate::{Attached, AttachedDescriptor, Provenance, Registry, RegistryError, Resolved};

pub(crate) const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
pub(crate) const CONTROL_PLANE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane";
pub(crate) const CONSOLE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui";

/// Every role, and where it is published.
pub(crate) const ROLES: [(&str, &str); 3] = [
    ("console", CONSOLE),
    ("controlPlane", CONTROL_PLANE),
    ("runtime", RUNTIME),
];

/// The roles as an environment pins them.
pub(crate) fn pins() -> BTreeMap<String, String> {
    ROLES
        .iter()
        .map(|(role, repository)| ((*role).to_owned(), (*repository).to_owned()))
        .collect()
}

/// A well-formed digest, the same for the same seed and different otherwise.
pub(crate) fn digest(seed: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("sha256:{}", format!("{:016x}", hasher.finish()).repeat(4))
}

/// The digest `role`'s image of `version` is published at.
pub(crate) fn image_digest(version: &str, role: &str) -> String {
    digest(&format!("{version}/{role}"))
}

/// The digest of `version`'s component descriptor manifest.
pub(crate) fn descriptor_digest(version: &str) -> String {
    digest(&format!("{version}/component descriptor"))
}

/// What `version`'s component descriptor says, naming every role at the
/// digest it is published at.
pub(crate) fn spec(version: &str) -> ComponentSpec {
    ComponentSpec {
        name: ComponentName::try_new("saas-fabric").unwrap(),
        title: "SaaS Fabric".to_owned(),
        description: String::new(),
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
        fields: Vec::new(),
        resources: Vec::new(),
    }
}

/// `spec` as it is attached: v1, annotated with `revision` and its version.
pub(crate) fn attached(version: &str, spec: ComponentSpec, revision: &str) -> Attached {
    Attached::One(AttachedDescriptor {
        digest: descriptor_digest(version),
        artifact_type: ARTIFACT_TYPE.to_owned(),
        revision: Some(revision.to_owned()),
        version: Some(version.to_owned()),
        document: ComponentDescriptor::new(spec).unwrap().to_json(),
    })
}

/// A registry whose contents a test sets, and can change between reads.
#[derive(Default)]
pub(crate) struct FakeRegistry {
    /// `(repository, tag)` to the digest it points at.
    tags: Mutex<BTreeMap<(String, String), String>>,
    /// `(repository, tag)` listed and not resolvable: gone since listing.
    listed_only: Mutex<BTreeSet<(String, String)>>,
    /// `(repository, digest)` to what the manifest says it came from.
    manifests: Mutex<BTreeMap<(String, String), Provenance>>,
    /// `(repository, subject)` to what is attached.
    attached: Mutex<BTreeMap<(String, String), Attached>>,
    /// References whose read fails, as a registry that cannot be asked.
    failing: Mutex<BTreeSet<String>>,
}

impl FakeRegistry {
    /// A manifest at `digest`, tagged `tag`.
    pub(crate) fn image(&self, repository: &str, tag: &str, digest: &str, provenance: Provenance) {
        self.untagged(repository, digest, provenance);
        self.tag(repository, tag, digest);
    }

    /// A manifest at `digest`, with no tag.
    pub(crate) fn untagged(&self, repository: &str, digest: &str, provenance: Provenance) {
        lock(&self.manifests).insert((repository.to_owned(), digest.to_owned()), provenance);
    }

    /// Points `tag` at `digest`.
    pub(crate) fn tag(&self, repository: &str, tag: &str, digest: &str) {
        lock(&self.tags).insert((repository.to_owned(), tag.to_owned()), digest.to_owned());
    }

    /// Removes `tag`.
    pub(crate) fn untag(&self, repository: &str, tag: &str) {
        lock(&self.tags).remove(&(repository.to_owned(), tag.to_owned()));
    }

    /// Removes the manifest at `digest`, leaving any tag pointing at nothing.
    pub(crate) fn delete(&self, repository: &str, digest: &str) {
        lock(&self.manifests).remove(&(repository.to_owned(), digest.to_owned()));
    }

    /// Lists `tag` without it resolving: a tag deleted after the listing.
    pub(crate) fn list_only(&self, repository: &str, tag: &str) {
        lock(&self.listed_only).insert((repository.to_owned(), tag.to_owned()));
    }

    /// Attaches `attached` to `subject`.
    pub(crate) fn attach(&self, repository: &str, subject: &str, attached: Attached) {
        lock(&self.attached).insert((repository.to_owned(), subject.to_owned()), attached);
    }

    /// Makes every read of `reference` fail.
    pub(crate) fn fail_on(&self, reference: &str) {
        lock(&self.failing).insert(reference.to_owned());
    }

    /// Every image of `version`, from `revision`, tagged with it.
    pub(crate) fn publish_images(&self, version: &str, revision: &str) {
        for (role, repository) in ROLES {
            let provenance = Provenance::Agreed(revision.to_owned());
            self.image(repository, version, &image_digest(version, role), provenance);
        }
    }

    /// A whole release: every image, and then its component descriptor.
    pub(crate) fn publish(&self, version: &str, revision: &str) {
        self.publish_images(version, revision);
        self.describe(version, attached(version, spec(version), revision));
    }

    /// Attaches `attached` to `version`'s runtime image.
    pub(crate) fn describe(&self, version: &str, attached: Attached) {
        self.attach(RUNTIME, &image_digest(version, "runtime"), attached);
    }

    /// Fails if `reference` was set to fail.
    fn check(&self, reference: &str) -> Result<(), RegistryError> {
        if lock(&self.failing).contains(reference) {
            return Err(RegistryError::Unavailable {
                detail: format!("{reference} could not be read"),
            });
        }
        Ok(())
    }
}

/// A lock, whether or not a test panicked holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait::async_trait]
impl Registry for FakeRegistry {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        let mut tags: BTreeSet<String> = lock(&self.tags)
            .keys()
            .chain(lock(&self.listed_only).iter())
            .filter(|(published, _)| published == repository)
            .map(|(_, tag)| tag.clone())
            .collect();
        // What a real registry lists beside the versions: the referrers tag
        // schema's tags, which are not versions and must drop out.
        tags.insert(format!("sha256-{}", "0".repeat(64)));
        Ok(tags.into_iter().collect())
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        self.check(reference)?;
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
        self.check(subject)?;
        Ok(lock(&self.attached)
            .get(&(repository.to_owned(), subject.to_owned()))
            .cloned()
            .unwrap_or(Attached::Nothing))
    }
}
