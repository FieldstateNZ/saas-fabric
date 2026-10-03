//! A registry holding a described component (ADR 0026), arranged by hand:
//! tags, manifests by digest with their revisions, and what is attached to
//! each primary image.
//!
//! Every answer is from memory. No test here reaches a network, and a
//! component descriptor is written as the JSON a publisher attaches -- not
//! built through `fabric-component`, which this crate does not depend on --
//! so the wire shape the rule reads is what these tests hand it.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, PoisonError};

use fabric_platform_management::{
    ArtifactSource, Attached, AttachedDescriptor, Channel, ComponentDesired, DesiredRevision, Provenance,
    Registry, RegistryError, Resolved, UpdatePolicy, Version,
};

/// The component every test here reads.
pub const COMPONENT: &str = "saas-fabric";

/// The primary image's repository: where the component descriptor is
/// attached, and whose tags are the versions there are.
pub const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";

/// The other image's repository.
pub const CONTROL_PLANE: &str = "ghcr.io/fieldstatenz/saas-fabric-control-plane";

/// Both roles, and where each is published.
const ROLES: [(&str, &str); 2] = [("controlPlane", CONTROL_PLANE), ("runtime", RUNTIME)];

/// A v1 component descriptor's artifact type, as a publisher writes it.
const ARTIFACT_TYPE: &str = "application/vnd.saas-fabric.component.v1";

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

/// Desired state for [`COMPONENT`], described, at `version` on the preview
/// channel, pinning both roles with `runtime` as the primary.
pub fn described_at(version: &str) -> (String, ComponentDesired) {
    let desired = ComponentDesired {
        version: Version::parse(version).expect("the fixture version parses"),
        channel: Channel::Preview,
        policy: UpdatePolicy::Manual,
        hold: None,
        source: ArtifactSource::Described {
            primary: "runtime".to_owned(),
            repositories: ROLES
                .iter()
                .map(|(role, repository)| ((*role).to_owned(), (*repository).to_owned()))
                .collect(),
        },
        revision: DesiredRevision::new("desired-1"),
    };

    (COMPONENT.to_owned(), desired)
}

/// `version`'s component descriptor as it is attached: v1, naming both
/// images at the digests they are published at, annotated with `revision`
/// and the version.
pub fn attached(version: &str, revision: &str) -> Attached {
    let images: serde_json::Map<String, serde_json::Value> = ROLES
        .iter()
        .map(|(role, repository)| {
            let image = serde_json::json!({
                "repository": repository,
                "digest": image_digest(version, role),
            });
            ((*role).to_owned(), image)
        })
        .collect();
    let document = serde_json::json!({
        "apiVersion": "fabric.fieldstate.nz/v1",
        "kind": "Component",
        "spec": {
            "name": COMPONENT,
            "title": "SaaS Fabric",
            "version": version,
            "images": images,
        },
    });

    Attached::One(AttachedDescriptor {
        digest: descriptor_digest(version),
        artifact_type: ARTIFACT_TYPE.to_owned(),
        revision: Some(revision.to_owned()),
        version: Some(version.to_owned()),
        document: serde_json::to_vec(&document).expect("the fixture document serializes"),
    })
}

/// A registry whose contents a test sets.
#[derive(Default)]
pub struct DescribedRegistry {
    /// `(repository, tag)` to the digest it points at.
    tags: Mutex<BTreeMap<(String, String), String>>,

    /// `(repository, digest)` to the revision its manifest carries.
    manifests: Mutex<BTreeMap<(String, String), Provenance>>,

    /// Primary image digest to what is attached to it.
    attached: Mutex<BTreeMap<String, Attached>>,
}

impl DescribedRegistry {
    /// `role`'s image of `version`, tagged with it and built from
    /// `revision`.
    pub fn publish_image(&self, version: &str, role: &str, revision: &str) {
        let (_, repository) = ROLES
            .iter()
            .find(|(named, _)| *named == role)
            .expect("the fixture names this role");
        let digest = image_digest(version, role);

        lock(&self.manifests).insert(
            ((*repository).to_owned(), digest.clone()),
            Provenance::Agreed(revision.to_owned()),
        );
        lock(&self.tags).insert(((*repository).to_owned(), version.to_owned()), digest);
    }

    /// Every image of `version`, each tagged with it and built from the
    /// revision `revision_of` gives its role.
    pub fn publish_images(&self, version: &str, revision_of: impl Fn(&str) -> String) {
        for (role, _) in ROLES {
            self.publish_image(version, role, &revision_of(role));
        }
    }

    /// Attaches `attached` to `version`'s runtime image.
    pub fn describe(&self, version: &str, attached: Attached) {
        lock(&self.attached).insert(image_digest(version, "runtime"), attached);
    }

    /// A whole release, every image and its component descriptor from
    /// `revision`.
    pub fn publish(&self, version: &str, revision: &str) {
        self.publish_images(version, |_| revision.to_owned());
        self.describe(version, attached(version, revision));
    }
}

/// A lock, whether or not a test panicked holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[async_trait::async_trait]
impl Registry for DescribedRegistry {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        let tags: BTreeSet<String> = lock(&self.tags)
            .keys()
            .filter(|(published, _)| published == repository)
            .map(|(_, tag)| tag.clone())
            .collect();
        Ok(tags.into_iter().collect())
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
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
        if repository != RUNTIME {
            return Ok(Attached::Nothing);
        }
        Ok(lock(&self.attached)
            .get(subject)
            .cloned()
            .unwrap_or(Attached::Nothing))
    }
}
