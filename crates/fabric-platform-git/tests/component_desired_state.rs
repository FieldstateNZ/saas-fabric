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
    WantedVersion,
};
use fabric_platform_management::{DesiredRevision, DesiredState, DesiredStateError, Hold, Release, Version};

mod support;

use support::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};

const MANIFEST: &str = "environments/lucentroot/components.yaml";
const RUNTIME_OVERLAY: &str = "applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml";
const OPERATOR_OVERLAY: &str =
    "applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml";

const CHART_REPOSITORY: &str = "https://codecentric.github.io/helm-charts";
const CHART: &str = "keycloakx";
const APPLICATION: &str = "applications/core/keycloak/application.yaml";

/// An Argo Application shaped like the ones this platform deploys, in the
/// canonical shape `components::argo_tests::APPLICATION` documents: the
/// chart, and the repository holding its values, which carries a
/// `targetRevision` of its own that a chart pin must never touch.
const APPLICATION_TEXT: &str = r"spec:
  sources:
    # The upstream chart. Bumping this is a deliberate act.
    - repoURL: https://codecentric.github.io/helm-charts
      chart: keycloakx
      targetRevision: 7.3.0
      helm:
        releaseName: keycloak
    - repoURL: https://github.com/FieldstateNZ/saas-fabric-platform.git
      targetRevision: PLACEHOLDER
      ref: platform
";

/// The manifest as the platform repository writes it, header and all.
const MANIFEST_TEXT: &str = r"# What LucentRoot is asked to run, and the policy that moves it.
#
# Machine-managed. Editing it by hand is the break-glass path.
---
schemaVersion: 2
environment: lucentroot
managedRoots:
  - applications/
components:
  saas-fabric:
    artifact:
      type: oci
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
      version: 0.3.0-preview.1
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
fn preview_two() -> WantedVersion {
    WantedVersion::Images(preview_two_unit())
}

fn preview_two_unit() -> ComponentVersion {
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

/// The revision the manifest is at, which every write must present.
///
/// Read through the port, exactly as a decision would read it — so a test
/// writes against the state it decided from, and a test that wants a stale
/// decision has to construct one deliberately.
fn unread() -> DesiredRevision {
    // For a manifest this build cannot read at all. The write refuses on the
    // read long before the revision is compared, so what it says is immaterial
    // -- and pretending to have read one would be the lie.
    DesiredRevision::new("never-read")
}

async fn at(repository: &PlatformGitRepository) -> DesiredRevision {
    revision_of(repository, "lucentroot", "saas-fabric").await
}

/// The revision of any component, read through the port. `at` above is the
/// one every OCI test wants; this is for the Helm component tests, which do
/// not share its name.
async fn revision_of(
    repository: &PlatformGitRepository,
    environment: &str,
    component: &str,
) -> DesiredRevision {
    repository
        .component(environment, component)
        .await
        .expect("the component reads")
        .revision
}

/// A `components.yaml` describing one Helm component: its artifact identity,
/// its desired version, and every place it is pinned. So a test naming what
/// it actually varies -- a chart, a repository, a pin -- does not have to
/// spell out the manifest surrounding it, the way the giant `.replace` calls
/// on `MANIFEST_TEXT` do for the OCI component.
fn helm_manifest(
    component: &str,
    version: &str,
    repository: &str,
    chart: &str,
    pins: &[(&str, &str, &str)],
) -> String {
    use std::fmt::Write as _;

    let pinned_in = if pins.is_empty() {
        "    pinnedIn: []\n".to_owned()
    } else {
        let mut out = "    pinnedIn:\n".to_owned();
        for (path, pin_repository, pin_chart) in pins {
            let _ = write!(
                out,
                "      - renderer: argo-target-revision\n        path: {path}\n        repository: {pin_repository}\n        chart: {pin_chart}\n"
            );
        }
        out
    };

    format!(
        r"schemaVersion: 2
environment: lucentroot
managedRoots:
  - applications/
components:
  {component}:
    artifact:
      type: helm
      repository: {repository}
      chart: {chart}
    channel: stable
    update: manual
    desired:
      version: {version}
{pinned_in}    hold: null
"
    )
}

#[tokio::test]
async fn a_promotion_moves_the_manifest_and_every_pin_in_one_commit() {
    let host = host().await;
    let repository = repository(&host);

    repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote LucentRoot",
        )
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
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote LucentRoot",
        )
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
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote LucentRoot",
        )
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
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote LucentRoot",
        )
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
async fn a_hold_created_after_the_decision_stops_the_write() {
    // The race that matters once humans and automation both write this file.
    // A selector reads `automatic, no hold, desired preview.1` and decides to
    // advance; before it writes, an operator commits a hold. The write must
    // not land on the strength of a policy view that is no longer true.
    //
    // Nothing here consults the policy. The manifest is one of the files being
    // written, so its revision is part of the write's own precondition -- and
    // a decision made against an older manifest cannot be applied to a newer
    // one. That is the same mechanism that refuses a concurrent version edit,
    // and it covers this without a second check to keep in step.
    let host = host().await;
    let repository = repository(&host);

    let held = MANIFEST_TEXT.replace(
        "    hold: null\n",
        "    hold:\n      reason: rollback\n      since: 2026-09-01T09:00:00Z\n",
    );
    host.someone_else_commits(&[(MANIFEST, &held)]);

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote LucentRoot",
        )
        .await
        .expect_err("a stale policy view must not carry across a concurrent change");

    assert_eq!(
        failure,
        PlatformGitError::Conflict {
            path: MANIFEST.to_owned()
        }
    );

    // And the hold the operator wrote is still there, untouched.
    let manifest = host.current(MANIFEST).unwrap();
    assert!(manifest.contains("reason: rollback"), "{manifest}");
    assert!(
        manifest.contains("version: 0.3.0-preview.1"),
        "desired state moved anyway"
    );

    // The overlays did not move either. A partial promotion would be worse
    // than the refused one.
    assert!(host.current(RUNTIME_OVERLAY).unwrap().contains("0.3.0-preview.1"));
    assert!(host
        .current(OPERATOR_OVERLAY)
        .unwrap()
        .contains("0.3.0-preview.1"));
}

#[tokio::test]
async fn two_thirds_of_a_release_is_refused() {
    let host = host().await;
    let repository = repository(&host);

    let mut partial = preview_two_unit();
    partial.images.remove("console");

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &WantedVersion::Images(partial),
            &at(&repository).await,
            "Promote LucentRoot",
        )
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

    let mut elsewhere = preview_two_unit();
    elsewhere.images.get_mut("runtime").unwrap().repository = "ghcr.io/attacker/saas-fabric".to_owned();

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &WantedVersion::Images(elsewhere),
            &at(&repository).await,
            "Promote LucentRoot",
        )
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
            .set_component_desired_state(
                "lucentroot",
                "saas-fabric",
                &preview_two(),
                &at(&repository).await,
                "Promote",
            )
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

    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote",
        )
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
    // The runtime's pin, pointed at the overlay that carries the *other* two
    // images. The file exists and is writable; it simply does not name this
    // image, and guessing which entry was meant is how half a promotion lands.
    let manifest = MANIFEST_TEXT.replace(
        "        path: applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml",
        "        path: applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml",
    );

    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;
    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote",
        )
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
            .set_component_desired_state(
                environment,
                component,
                &preview_two(),
                &at(&repository).await,
                "Promote",
            )
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
    let manifest = MANIFEST_TEXT.replace("schemaVersion: 2", "schemaVersion: 3");
    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;

    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), &unread(), "Promote")
        .await
        .expect_err("a shape this was not written against is not read optimistically");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_kustomize_pin_that_names_no_image_does_not_even_parse() {
    // Not rejected downstream -- unrepresentable. `image` is a field of the
    // `kustomize-image` variant rather than an optional field beside a
    // renderer name, so a pin without one is not a pin this schema can
    // describe. Nothing downstream has to check for it.
    //
    // The failure it prevents: the control-plane overlay pins two images, so a
    // pin that did not say which would have to guess between them.
    let manifest = MANIFEST_TEXT.replace("        image: runtime\n", "");

    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;

    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), &unread(), "Promote")
        .await
        .expect_err("a pin that names no image cannot be written");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_pin_naming_an_image_the_component_does_not_publish_is_refused() {
    // The manifest disagreeing with itself. Nothing is written, because the
    // alternative is deciding on the operator's behalf which image they meant.
    let manifest = MANIFEST_TEXT.replace("        image: runtime", "        image: sidecar");

    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;

    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote",
        )
        .await
        .expect_err("an image the component does not publish cannot be pinned");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn an_unknown_renderer_is_refused_rather_than_guessed_at() {
    // The field that would ruin this design if it grew a general escape. The
    // renderer is the variant tag, so one this build does not implement names
    // no variant and the document does not parse -- rather than parsing into
    // something with a renderer field nobody matched on.
    let manifest = MANIFEST_TEXT.replace("renderer: kustomize-image", "renderer: json-path");

    let host = FakePlatformHost::start(&[
        (MANIFEST, &manifest),
        (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT),
        (OPERATOR_OVERLAY, OPERATOR_OVERLAY_TEXT),
    ])
    .await;

    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), &unread(), "Promote")
        .await
        .expect_err("a renderer this build does not know is not a renderer");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn an_older_schema_is_named_as_a_version_rather_than_a_missing_field() {
    // The diagnostic during a schema migration, which is the one moment it
    // matters. A version 1 manifest read by a version 2 build fails on
    // whichever field moved unless the version is checked first -- so the
    // operator is told about an unknown key at exactly the moment they need to
    // be told the file is a version behind.
    let manifest = MANIFEST_TEXT
        .replace("schemaVersion: 2", "schemaVersion: 1")
        .replace(
            "    artifact:\n      type: oci\n      sourceRevision:",
            "    legacyShape:\n      whatever:",
        );

    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;

    let failure = repository(&host)
        .components_manifest("lucentroot")
        .await
        .expect_err("a manifest from another schema is not read");

    let PlatformGitError::Rejected { detail } = failure else {
        panic!("a schema mismatch is a refusal");
    };

    assert!(
        detail.contains("schemaVersion 1") && detail.contains("reads 2"),
        "the version is what it names: {detail}"
    );
}

#[tokio::test]
async fn a_decision_taken_against_state_that_has_since_moved_is_refused() {
    // The gap this closes. A sweep reads desired state, decides, and writes --
    // and between the read and the write an operator adds a hold. The write
    // used to re-read and apply the decision to whatever it found, so the hold
    // was silently ignored: the decision was right when it was taken and wrong
    // by the time it landed.
    //
    // The revision the decision was read at is now the write's precondition,
    // so state that moved in between is a conflict.
    let host = host().await;
    let repository = repository(&host);

    let stale = at(&repository).await;

    // Somebody else writes. Any write moves the manifest's revision.
    let held = MANIFEST_TEXT.replace(
        "    hold: null",
        "    hold:\n      reason: paused\n      since: 2026-09-04T09:00:00Z",
    );
    host.someone_else_commits(&[(MANIFEST, &held)]);

    let failure = repository
        .set_component_desired_state("lucentroot", "saas-fabric", &preview_two(), &stale, "Promote")
        .await
        .expect_err("a decision about the old state cannot be applied to the new one");

    assert!(
        matches!(failure, PlatformGitError::Conflict { .. }),
        "moved state is a conflict, not an overwrite: {failure:?}"
    );
}

#[tokio::test]
async fn the_same_decision_applies_cleanly_when_nothing_has_moved() {
    // The other half, and the one that keeps the test above honest: if a
    // fresh revision were also refused, the check would be rejecting
    // everything rather than rejecting staleness.
    let host = host().await;
    let repository = repository(&host);

    repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &preview_two(),
            &at(&repository).await,
            "Promote",
        )
        .await
        .expect("a decision taken against current state applies");
}

#[tokio::test]
async fn a_chart_release_must_agree_with_the_artifact_and_the_pin() {
    // What the component is published as and what the file pins must both
    // agree with a release before it is written. A release discovered from
    // one chart written into a pin for another would deploy plausible-looking
    // wrong software.
    let manifest = helm_manifest(
        "saas-fabric",
        "0.3.0-preview.1",
        "https://charts.example.test",
        "keycloakx",
        &[(RUNTIME_OVERLAY, "https://charts.example.test", "keycloakx")],
    );

    let host =
        FakePlatformHost::start(&[(MANIFEST, &manifest), (RUNTIME_OVERLAY, RUNTIME_OVERLAY_TEXT)]).await;
    let repository = repository(&host);

    // A release of a *different* chart, which the manifest does not publish.
    let elsewhere = WantedVersion::Chart {
        repository: "https://charts.example.test".to_owned(),
        chart: "postgresql".to_owned(),
        version: "16.0.0".to_owned(),
    };

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &elsewhere,
            &revision_of(&repository, "lucentroot", "saas-fabric").await,
            "Promote",
        )
        .await
        .expect_err("a release of another chart is not this component's release");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_chart_version_with_build_metadata_is_read_back_after_being_advanced_to() {
    // The gap this closes: `component` used to parse every artifact's
    // `desired.version` with the OCI grammar, which rejects `+`. Discovery
    // finds a chart's build metadata, `advance` writes it verbatim, and the
    // very next read of that component refused it -- a version this build had
    // itself just written became unreadable.
    let manifest = helm_manifest(
        "keycloak",
        "7.3.0",
        CHART_REPOSITORY,
        CHART,
        &[(APPLICATION, CHART_REPOSITORY, CHART)],
    );
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest), (APPLICATION, APPLICATION_TEXT)]).await;
    let repository = repository(&host);

    let release = Release::Chart {
        repository: CHART_REPOSITORY.to_owned(),
        chart: CHART.to_owned(),
        version: Version::parse_chart("7.3.1+build.7").expect("a chart version may carry build metadata"),
    };

    repository
        .advance(
            "lucentroot",
            "keycloak",
            &release,
            &revision_of(&repository, "lucentroot", "keycloak").await,
            "Promote",
        )
        .await
        .expect("a chart version with build metadata can be advanced to");

    assert_eq!(host.ref_updates(), 1, "one change is one commit");

    let manifest = host.current(MANIFEST).unwrap();
    assert!(manifest.contains("version: 7.3.1+build.7"), "{manifest}");

    let application = host.current(APPLICATION).unwrap();
    assert!(
        application.contains("targetRevision: 7.3.1+build.7"),
        "{application}"
    );
    assert!(
        application.contains("targetRevision: PLACEHOLDER"),
        "the platform repository's own source is not a chart and must not move:\n{application}"
    );

    let read = repository
        .component("lucentroot", "keycloak")
        .await
        .expect("a version this build just wrote is a version it can read back");

    assert_eq!(read.version.as_str(), "7.3.1+build.7");
}

#[tokio::test]
async fn an_image_version_with_build_metadata_is_refused_on_read() {
    // The asymmetry that makes the round trip above meaningful: this is not a
    // relaxed rule for everyone, it is the artifact's grammar. An OCI tag
    // cannot carry `+`, so an image component whose `desired.version`
    // somehow carries build metadata is refused rather than read as if the
    // `+` were not there.
    let manifest = MANIFEST_TEXT.replace("version: 0.3.0-preview.1", "version: 0.3.0-preview.1+build");
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let failure = repository
        .component("lucentroot", "saas-fabric")
        .await
        .expect_err("an image version cannot carry build metadata");

    assert!(
        matches!(failure, DesiredStateError::Refused { .. }),
        "{failure:?}"
    );
}

#[tokio::test]
async fn a_helm_component_cannot_be_asked_to_accept_container_images() {
    // Before `check_release` covered every combination, it returned `Ok(())`
    // for every (artifact, release) pair except (Oci, Images). With
    // `pinnedIn: []` there was nothing for `rewrite_pins` to disagree with
    // either, so `apply` would have written the image release's version
    // string into a Helm component's `desired.version` regardless.
    let manifest = helm_manifest("keycloak", "7.3.0", CHART_REPOSITORY, CHART, &[]);
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "keycloak",
            &WantedVersion::Images(preview_two_unit()),
            &revision_of(&repository, "lucentroot", "keycloak").await,
            "Promote",
        )
        .await
        .expect_err("a Helm component cannot accept an image release");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
    assert!(host.current(MANIFEST).unwrap().contains("version: 7.3.0"));
}

#[tokio::test]
async fn an_oci_component_cannot_be_asked_to_accept_a_chart_release() {
    // The other half of the same hole: an OCI component with `pinnedIn: []`
    // offered a chart release used to reach `apply` unrejected too, and would
    // have ended up claiming a chart's version with no digest to match it.
    let manifest = MANIFEST_TEXT.replace(
        "    pinnedIn:\n      - renderer: kustomize-image\n        path: applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml\n        image: console\n      - renderer: kustomize-image\n        path: applications/core/saas-fabric-control-plane/overlays/lucentroot/kustomization.yaml\n        image: controlPlane\n      - renderer: kustomize-image\n        path: applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml\n        image: runtime\n",
        "    pinnedIn: []\n",
    );
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let chart_release = WantedVersion::Chart {
        repository: CHART_REPOSITORY.to_owned(),
        chart: CHART.to_owned(),
        version: "7.3.1".to_owned(),
    };

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "saas-fabric",
            &chart_release,
            &at(&repository).await,
            "Promote",
        )
        .await
        .expect_err("an OCI component cannot accept a chart release");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_helm_release_must_name_the_chart_the_component_publishes() {
    // A different chart from the same repository. Nothing here pins
    // anything, so before the exact-identity rule this fell through to
    // `Ok(())` and `apply` recorded the wrong chart's version as this
    // component's own.
    let manifest = helm_manifest("keycloak", "7.3.0", CHART_REPOSITORY, CHART, &[]);
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let elsewhere = WantedVersion::Chart {
        repository: CHART_REPOSITORY.to_owned(),
        chart: "postgresql".to_owned(),
        version: "16.0.0".to_owned(),
    };

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "keycloak",
            &elsewhere,
            &revision_of(&repository, "lucentroot", "keycloak").await,
            "Promote",
        )
        .await
        .expect_err("a different chart in the same repository is not this component's release");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_helm_release_must_come_from_the_repository_the_component_publishes_from() {
    // The same chart name, from a different repository. Matching on the
    // chart name alone would let this through, and the difference between
    // the two repositories is which software gets deployed.
    let manifest = helm_manifest("keycloak", "7.3.0", CHART_REPOSITORY, CHART, &[]);
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let elsewhere = WantedVersion::Chart {
        repository: "https://charts.example.test".to_owned(),
        chart: CHART.to_owned(),
        version: "7.3.1".to_owned(),
    };

    let failure = repository
        .set_component_desired_state(
            "lucentroot",
            "keycloak",
            &elsewhere,
            &revision_of(&repository, "lucentroot", "keycloak").await,
            "Promote",
        )
        .await
        .expect_err("the same chart name from a different repository is different software");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}

#[tokio::test]
async fn a_helm_release_that_matches_exactly_is_written_even_with_no_pins() {
    // The two tests above prove the rule rejects a mismatch; this proves it
    // is not simply rejecting every chart release -- an exact match still
    // writes, even though there is no pin file for `rewrite_pins` to touch.
    let manifest = helm_manifest("keycloak", "7.3.0", CHART_REPOSITORY, CHART, &[]);
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let matching = WantedVersion::Chart {
        repository: CHART_REPOSITORY.to_owned(),
        chart: CHART.to_owned(),
        version: "7.3.1".to_owned(),
    };

    repository
        .set_component_desired_state(
            "lucentroot",
            "keycloak",
            &matching,
            &revision_of(&repository, "lucentroot", "keycloak").await,
            "Promote",
        )
        .await
        .expect("an exact match is this component's release");

    assert_eq!(host.ref_updates(), 1);
    assert!(host.current(MANIFEST).unwrap().contains("version: 7.3.1"));
}

#[tokio::test]
async fn a_helm_component_cannot_be_rolled_back_to_an_image_release() {
    // `roll_back_component` always wraps its unit in `WantedVersion::Images`,
    // so this is the same hole as `advance`, reached from the rollback path
    // instead. The service layer refuses this earlier, via `rollable()` --
    // but the adapter must refuse it too, rather than trust every caller
    // above it to have asked first.
    let manifest = helm_manifest("keycloak", "7.3.0", CHART_REPOSITORY, CHART, &[]);
    let host = FakePlatformHost::start(&[(MANIFEST, &manifest)]).await;
    let repository = repository(&host);

    let hold = Hold {
        reason: "rollback".to_owned(),
        since: "2026-09-04T09:00:00Z".to_owned(),
        note: None,
    };

    let failure = repository
        .roll_back_component(
            "lucentroot",
            "keycloak",
            &preview_two_unit(),
            &hold,
            &revision_of(&repository, "lucentroot", "keycloak").await,
            "Roll back",
        )
        .await
        .expect_err("a Helm component cannot be rolled back to an image release");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
    assert_eq!(host.ref_updates(), 0);
}
