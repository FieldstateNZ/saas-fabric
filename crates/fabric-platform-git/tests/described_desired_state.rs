//! A described component (schema 3, ADR 0026 section 9) moves as images do:
//! the manifest and every pin in one commit, its primary kept, and nothing
//! it was not asked to write.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_core::Clock;
use fabric_git_host::GitCredential;
use fabric_platform_git::{
    ComponentVersion, ImageDigest, PlatformGitError, PlatformGitRepository, PlatformRepositoryConfig,
    WantedVersion,
};
use fabric_platform_management::{
    ArtifactSource, DesiredRevision, DesiredState, Hold, Release, ReleaseUnit, ResolvedImage, Version,
};

mod support;

use support::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};

const MANIFEST: &str = "environments/lucentroot/components.yaml";
const RUNTIME_OVERLAY: &str = "applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml";
const OPERATOR_OVERLAY: &str =
    "applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml";

const MANIFEST_TEXT: &str = r"# What LucentRoot is asked to run, and the policy that moves it.
#
# Machine-managed. Editing it by hand is the break-glass path.
---
schemaVersion: 3
environment: lucentroot
managedRoots:
  - applications/
components:
  saas-fabric:
    artifact:
      type: described
      primary: runtime
      sourceRevision: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      images:
        console:
          repository: ghcr.io/fieldstatenz/saas-fabric-control-plane-ui
          digest: sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
        controlPlane:
          repository: ghcr.io/fieldstatenz/saas-fabric-control-plane
          digest: sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
        runtime:
          repository: ghcr.io/fieldstatenz/saas-fabric
          digest: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    channel: preview
    update: automatic
    desired:
      version: 0.3.0-preview.2
    pinnedIn:
      - renderer: kustomize-image
        path: applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml
        image: console
      - renderer: kustomize-image
        path: applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml
        image: controlPlane
      - renderer: kustomize-image
        path: applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml
        image: runtime
    hold: null
";

const RUNTIME_OVERLAY_TEXT: &str = r"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

# Pinned. The replica count in the base is what holds this at zero.
images:
  - name: ghcr.io/fieldstatenz/saas-fabric
    newTag: 0.3.0-preview.2
    digest: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
";

const OPERATOR_OVERLAY_TEXT: &str = r"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

# Pinned, never `latest`.
images:
  - name: ghcr.io/fieldstatenz/saas-fabric-control-plane
    newTag: 0.3.0-preview.2
    digest: sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  - name: ghcr.io/fieldstatenz/saas-fabric-control-plane-ui
    newTag: 0.3.0-preview.2
    digest: sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
";

struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_787_907_600
    }
}

fn repository(host: &FakePlatformHost) -> PlatformGitRepository {
    PlatformGitRepository::new(
        &PlatformRepositoryConfig {
            api_base_url: host.base_url.clone(),
            owner: OWNER.to_owned(),
            repository: REPOSITORY.to_owned(),
            branch: BRANCH.to_owned(),
            http_timeout_seconds: 5,
            operation_timeout_seconds: 30,
        },
        GitCredential::token("test-bearer"),
        Arc::new(TestClock),
    )
    .unwrap()
}

async fn host_with(manifest: &str) -> FakePlatformHost {
    FakePlatformHost::start(&[
        (MANIFEST, manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await
}

async fn at(repository: &PlatformGitRepository) -> DesiredRevision {
    repository
        .component("lucentroot", "saas-fabric")
        .await
        .expect("the component reads")
        .revision
}

/// `version` of saas-fabric as discovery would hand it over: every image at
/// a digest made of `digit`, found through `primary`.
fn described(version: &str, digit: char, primary: &str) -> Release {
    let images = [
        ("console", "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui"),
        ("controlPlane", "ghcr.io/fieldstatenz/saas-fabric-control-plane"),
        ("runtime", "ghcr.io/fieldstatenz/saas-fabric"),
    ]
    .into_iter()
    .map(|(role, repository)| {
        (
            role.to_owned(),
            ResolvedImage {
                repository: repository.to_owned(),
                digest: format!("sha256:{}", digit.to_string().repeat(64)),
            },
        )
    })
    .collect();

    Release::Described {
        unit: ReleaseUnit {
            version: Version::parse(version).unwrap(),
            source_revision: digit.to_string().repeat(40),
            images,
        },
        primary: primary.to_owned(),
        descriptor: format!("sha256:{}", "d".repeat(64)),
    }
}

fn rollback_hold() -> Hold {
    Hold {
        reason: "rollback".to_owned(),
        since: "2026-09-29T09:00:00Z".to_owned(),
        note: None,
    }
}

#[tokio::test]
async fn a_described_component_is_read_as_described_with_its_pins() {
    let host = host_with(MANIFEST_TEXT).await;

    let desired = repository(&host)
        .component("lucentroot", "saas-fabric")
        .await
        .unwrap();

    let ArtifactSource::Described {
        primary,
        repositories,
    } = desired.source
    else {
        panic!("a described artifact reads as one");
    };
    assert_eq!(primary, "runtime");
    assert_eq!(repositories["runtime"], "ghcr.io/fieldstatenz/saas-fabric");
    assert_eq!(repositories.len(), 3);
    assert_eq!(desired.version.as_str(), "0.3.0-preview.2");
}

#[tokio::test]
async fn advancing_a_described_component_writes_the_images_and_the_commit_in_one_commit() {
    let host = host_with(MANIFEST_TEXT).await;
    let repository = repository(&host);

    repository
        .advance(
            "lucentroot",
            "saas-fabric",
            &described("0.3.0-preview.3", 'e', "runtime"),
            &at(&repository).await,
            "Advance lucentroot to saas-fabric 0.3.0-preview.3",
        )
        .await
        .expect("a described release for this component is written");

    assert_eq!(host.ref_updates(), 1, "one change is one commit");

    let manifest = host.current(MANIFEST).unwrap();
    assert!(manifest.contains("schemaVersion: 3"), "{manifest}");
    assert!(manifest.contains("type: described"), "{manifest}");
    assert!(
        manifest.contains("primary: runtime"),
        "the primary is kept: {manifest}"
    );
    assert!(manifest.contains("version: 0.3.0-preview.3"), "{manifest}");
    assert!(
        manifest.contains(&format!("sourceRevision: {}", "e".repeat(40))),
        "{manifest}"
    );
    assert_eq!(
        manifest.matches(&"e".repeat(64)).count(),
        3,
        "every digest: {manifest}"
    );
    assert!(
        !manifest.contains(&"d".repeat(64)),
        "the component descriptor's digest is not desired state"
    );

    for overlay in [RUNTIME_OVERLAY, OPERATOR_OVERLAY] {
        let text = host.current(overlay).unwrap();
        assert!(text.contains("newTag: 0.3.0-preview.3"), "{overlay}:\n{text}");
        assert!(text.contains(&"e".repeat(64)), "{overlay}:\n{text}");
        assert!(!text.contains("0.3.0-preview.2"), "{overlay} kept a stale pin");
    }
}

#[tokio::test]
async fn rolling_a_described_component_back_moves_it_and_holds_it_in_one_commit() {
    let host = host_with(MANIFEST_TEXT).await;
    let repository = repository(&host);

    repository
        .roll_back(
            "lucentroot",
            "saas-fabric",
            &described("0.3.0-preview.1", 'f', "runtime"),
            &rollback_hold(),
            &at(&repository).await,
            "Roll saas-fabric in lucentroot back to 0.3.0-preview.1",
        )
        .await
        .expect("a described component rolls back");

    assert_eq!(host.ref_updates(), 1, "the version and the hold are one commit");

    let manifest = host.current(MANIFEST).unwrap();
    assert!(manifest.contains("version: 0.3.0-preview.1"), "{manifest}");
    assert!(manifest.contains("reason: rollback"), "{manifest}");
    assert!(manifest.contains("primary: runtime"), "{manifest}");
    assert!(
        manifest.contains(&format!("sourceRevision: {}", "f".repeat(40))),
        "{manifest}"
    );
    assert_eq!(manifest.matches(&"f".repeat(64)).count(), 3, "{manifest}");

    let runtime = host.current(RUNTIME_OVERLAY).unwrap();
    assert!(runtime.contains("newTag: 0.3.0-preview.1"), "{runtime}");
    assert!(runtime.contains(&"f".repeat(64)), "{runtime}");
}

#[tokio::test]
async fn a_described_component_in_a_schema_2_file_is_refused_by_version() {
    let manifest = MANIFEST_TEXT.replace("schemaVersion: 3", "schemaVersion: 2");
    let host = host_with(&manifest).await;

    let failure = repository(&host)
        .components_manifest("lucentroot")
        .await
        .expect_err("described needs schema 3");

    let PlatformGitError::Rejected { detail } = failure else {
        panic!("a described component under schema 2 is a refusal");
    };
    assert!(
        detail.contains("schemaVersion 2") && detail.contains("needs schemaVersion 3"),
        "the versions are what it names: {detail}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_release_found_through_another_primary_is_refused() {
    let host = host_with(MANIFEST_TEXT).await;
    let repository = repository(&host);

    let failure = repository
        .advance(
            "lucentroot",
            "saas-fabric",
            &described("0.3.0-preview.3", 'e', "controlPlane"),
            &at(&repository).await,
            "Advance",
        )
        .await
        .expect_err("a release found through another image is not this component's");

    assert!(format!("{failure:?}").contains("controlPlane"), "{failure:?}");
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn images_by_role_and_a_described_release_are_not_each_other() {
    // Moving a component between being found by role and being found through
    // its component descriptor is an edit of the platform repository, never
    // a version.
    let host = host_with(MANIFEST_TEXT).await;
    let repository = repository(&host);

    let mut images = BTreeMap::new();
    for role in ["console", "controlPlane", "runtime"] {
        images.insert(
            role.to_owned(),
            ImageDigest {
                repository: String::new(),
                digest: format!("sha256:{}", "e".repeat(64)),
            },
        );
    }
    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &WantedVersion::Images(ComponentVersion {
                version: "0.3.0-preview.3".to_owned(),
                source_revision: "e".repeat(40),
                images,
            }),
            &at(&repository).await,
            "Advance",
        )
        .await
        .expect_err("images by role are not a described release");
    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );

    let oci = MANIFEST_TEXT.replace(
        "      type: described\n      primary: runtime\n",
        "      type: oci\n",
    );
    let host = host_with(&oci).await;
    let repository = self::repository(&host);
    let failure = repository
        .advance(
            "lucentroot",
            "saas-fabric",
            &described("0.3.0-preview.3", 'e', "runtime"),
            &at(&repository).await,
            "Advance",
        )
        .await
        .expect_err("a described release is not images by role");
    assert!(
        format!("{failure:?}").contains("component descriptor"),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}
