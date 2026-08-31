//! Moving a component's desired state means the manifest and every file that
//! carries a pin, in one commit, and nothing else.

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
};

mod support;

use support::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};

const MANIFEST: &str = "environments/lucentroot/components.yaml";
const RUNTIME_OVERLAY: &str = "applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml";
const OPERATOR_OVERLAY: &str =
    "applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml";

/// The manifest as the platform repository writes it, header and all.
const MANIFEST_TEXT: &str = r"# What LucentRoot is asked to run, and the policy that moves it.
#
# Machine-managed. Editing it by hand is the break-glass path.
---
schemaVersion: 1
environment: lucentroot
managedRoots:
  - applications/
components:
  saas-fabric:
    channel: preview
    update: automatic
    desired:
      version: 0.3.0-preview.1
      sourceRevision: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      images:
        console:
          repository: ghcr.io/fieldstatenz/saas-fabric-control-plane-ui
          digest: sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc
          pinnedIn:
            - applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml
        controlPlane:
          repository: ghcr.io/fieldstatenz/saas-fabric-control-plane
          digest: sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
          pinnedIn:
            - applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml
        runtime:
          repository: ghcr.io/fieldstatenz/saas-fabric
          digest: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
          pinnedIn:
            - applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml
    hold: null
";

/// A real overlay's shape: comments that explain the pin, and a patch after it.
const RUNTIME_OVERLAY_TEXT: &str = r"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  - ../../base

# Pinned. The replica count in the base is what holds this at zero.
images:
  - name: ghcr.io/fieldstatenz/saas-fabric
    newTag: 0.3.0-preview.1
    digest: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa

patches:
  - target:
      kind: HTTPRoute
      name: saas-fabric
    patch: |-
      - op: replace
        path: /spec/hostnames/0
        value: fabric.lucentroot.internal
";

/// The overlay two roles share.
const OPERATOR_OVERLAY_TEXT: &str = r"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  - ../../base

# Pinned, never `latest`.
images:
  - name: ghcr.io/fieldstatenz/saas-fabric-control-plane
    newTag: 0.3.0-preview.1
    digest: sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  - name: ghcr.io/fieldstatenz/saas-fabric-control-plane-ui
    newTag: 0.3.0-preview.1
    digest: sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc

configMapGenerator:
  - name: saas-fabric-control-plane-config
    behavior: replace
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
        },
        GitCredential::token("test-bearer"),
        Arc::new(TestClock),
    )
    .unwrap()
}

async fn host() -> FakePlatformHost {
    FakePlatformHost::start(&[
        (MANIFEST, MANIFEST_TEXT),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await
}

/// `preview.2`, complete and from one commit.
fn preview_two() -> ComponentVersion {
    let mut images = BTreeMap::new();
    for (role, repository, digest) in [
        (
            "console",
            "ghcr.io/fieldstatenz/saas-fabric-control-plane-ui",
            "3",
        ),
        (
            "controlPlane",
            "ghcr.io/fieldstatenz/saas-fabric-control-plane",
            "2",
        ),
        ("runtime", "ghcr.io/fieldstatenz/saas-fabric", "1"),
    ] {
        images.insert(
            role.to_owned(),
            ImageDigest {
                repository: repository.to_owned(),
                digest: format!("sha256:{}", digest.repeat(64)),
            },
        );
    }

    ComponentVersion {
        version: "0.3.0-preview.2".to_owned(),
        source_revision: "b".repeat(40),
        images,
    }
}

#[tokio::test]
async fn a_promotion_moves_the_manifest_and_every_pin_in_one_commit() {
    let host = host().await;
    let repository = repository(&host);

    repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote LucentRoot")
        .await
        .unwrap();

    assert_eq!(host.ref_updates(), 1, "one change is one commit");

    let manifest = host.current(MANIFEST).unwrap();
    assert!(manifest.contains("version: 0.3.0-preview.2"), "{manifest}");
    assert!(manifest.contains(&format!("sourceRevision: {}", "b".repeat(40))));

    for (overlay, digests) in [(RUNTIME_OVERLAY, vec!["1"]), (OPERATOR_OVERLAY, vec!["2", "3"])] {
        let text = host.current(overlay).unwrap();
        assert!(text.contains("newTag: 0.3.0-preview.2"), "{overlay}:\n{text}");
        for digest in digests {
            assert!(
                text.contains(&digest.repeat(64)),
                "{overlay} missing digest {digest}"
            );
        }
        assert!(!text.contains("0.3.0-preview.1"), "{overlay} kept a stale pin");
    }
}

#[tokio::test]
async fn an_overlay_keeps_everything_that_is_not_the_pin() {
    let host = host().await;
    let repository = repository(&host);

    repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote LucentRoot")
        .await
        .unwrap();

    let text = host.current(RUNTIME_OVERLAY).unwrap();

    // The comment explaining the pin, and the patch after it. A load-and-dump
    // would have deleted the first and reformatted the second.
    assert!(text.contains("# Pinned. The replica count in the base is what holds this at zero."));
    assert!(text.contains("value: fabric.lucentroot.internal"));
    assert!(
        text.contains("      - op: replace"),
        "the block scalar was reflowed:\n{text}"
    );

    let operator = host.current(OPERATOR_OVERLAY).unwrap();
    assert!(operator.contains("# Pinned, never `latest`."));
    assert!(operator.contains("behavior: replace"));
}

#[tokio::test]
async fn the_manifests_header_and_the_platforms_own_settings_survive() {
    let host = host().await;
    let repository = repository(&host);

    repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote LucentRoot")
        .await
        .unwrap();

    let manifest = host.current(MANIFEST).unwrap();

    // The header is the platform repository's words about its own file.
    assert!(
        manifest.starts_with("# What LucentRoot is asked to run"),
        "{manifest}"
    );
    assert!(manifest.contains("# Machine-managed. Editing it by hand is the break-glass path."));

    // Policy, channel and the layout declaration are not a caller's to move.
    assert!(manifest.contains("channel: preview"));
    assert!(manifest.contains("update: automatic"));
    assert!(manifest.contains("pinnedIn:"));
    assert!(manifest.contains(RUNTIME_OVERLAY));
}

#[tokio::test]
async fn a_hold_is_carried_through_rather_than_cleared() {
    // Advancing a held component is the selector's decision, not this one's --
    // but silently dropping the hold while writing would make that decision
    // impossible to enforce afterwards.
    let held = MANIFEST_TEXT.replace(
        "    hold: null\n",
        "    hold:\n      reason: rollback\n      since: 2026-09-01T09:00:00Z\n      note: preview.7 broke Secrets\n",
    );

    let host = FakePlatformHost::start(&[
        (MANIFEST, &held),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;
    let repository = repository(&host);

    repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote LucentRoot")
        .await
        .unwrap();

    let manifest = host.current(MANIFEST).unwrap();
    assert!(
        manifest.contains("reason: rollback"),
        "the hold was dropped:\n{manifest}"
    );
    assert!(manifest.contains("preview.7 broke Secrets"));
}

#[tokio::test]
async fn two_thirds_of_a_release_is_refused() {
    let host = host().await;
    let repository = repository(&host);

    let mut partial = preview_two();
    partial.images.remove("console");

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &partial, "Promote LucentRoot")
        .await
        .expect_err("a component's images move together");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
    assert!(host
        .current(MANIFEST)
        .unwrap()
        .contains("version: 0.3.0-preview.1"));
}

#[tokio::test]
async fn a_version_change_may_not_become_a_registry_change() {
    // Specification §31.8: a Set Version request does not get to say where an
    // artifact comes from. That is the platform repository's statement.
    let host = host().await;
    let repository = repository(&host);

    let mut elsewhere = preview_two();
    elsewhere.images.get_mut("runtime").unwrap().repository = "ghcr.io/attacker/saas-fabric".to_owned();

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &elsewhere, "Promote LucentRoot")
        .await
        .expect_err("the registry is not a caller's to choose");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_pin_declared_outside_the_managed_roots_is_refused() {
    for path in [
        ".github/workflows/release.yaml",
        "docs/architecture.yaml",
        "applications/core/saas-fabric/README.md",
        "applications/core/../../etc/passwd.yaml",
        "/etc/hosts.yaml",
    ] {
        let manifest = MANIFEST_TEXT.replace(RUNTIME_OVERLAY, path);
        let host = FakePlatformHost::start(&[
            (MANIFEST, &manifest),
            (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
            (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
        ])
        .await;
        let repository = repository(&host);

        let failure = repository
            .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote")
            .await
            .expect_err("a trusted manifest still does not get to name any file");

        assert!(
            matches!(failure, PlatformGitError::Rejected { .. }),
            "{path}: {failure:?}"
        );
        assert_eq!(host.ref_updates(), 0, "{path} reached a write");
    }
}

#[tokio::test]
async fn a_managed_root_cannot_open_the_whole_filesystem() {
    // The case that makes the absolute-path rule load-bearing rather than
    // belt-and-braces. Against the roots this repository declares, `/etc/...`
    // is already refused for being under none of them -- but a manifest that
    // declared `/` as a root would admit it, and then only the path's own
    // shape stands between a trusted mistake and writing outside the checkout.
    let manifest = MANIFEST_TEXT
        .replace("  - applications/", "  - /")
        .replace(RUNTIME_OVERLAY, "/etc/hosts.yaml");

    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;

    let failure = repository(&host)
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote")
        .await
        .expect_err("no manifest may widen this to the filesystem");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_pin_declared_in_a_file_that_does_not_carry_it_is_refused() {
    // The rule the platform repository's CI also enforces. It is applied again
    // here because Fabric reads whatever state exists, which may be one no CI
    // has seen.
    let manifest = MANIFEST_TEXT.replace(
        "            - applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml",
        "            - applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml",
    );

    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;
    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote")
        .await
        .expect_err("a file that does not pin the image cannot be repinned");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn an_unknown_component_or_environment_is_refused() {
    let host = host().await;
    let repository = repository(&host);

    for (environment, component) in [("lucentroot", "keycloak"), ("production", "saas-fabric")] {
        let failure = repository
            .set_component_desired_state(environment, component, &preview_two(), "Promote")
            .await
            .expect_err("only what the manifest declares may be moved");

        assert!(
            matches!(
                failure,
                PlatformGitError::Rejected { .. } | PlatformGitError::NotFound { .. }
            ),
            "{environment}/{component}: {failure:?}"
        );
    }

    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_manifest_from_a_newer_schema_is_refused_rather_than_guessed_at() {
    let manifest = MANIFEST_TEXT.replace("schemaVersion: 1", "schemaVersion: 2");
    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;

    let failure = repository(&host)
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), "Promote")
        .await
        .expect_err("a shape this was not written against is not read optimistically");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}
