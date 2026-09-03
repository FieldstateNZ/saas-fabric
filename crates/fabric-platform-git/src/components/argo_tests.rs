//! Which Argo source a chart pin is allowed to move.

use super::argo::retarget;
use crate::PlatformGitError;

/// An Application shaped like the ones this platform deploys: the chart, and
/// the repository holding its values, which carries a `targetRevision` of its
/// own.
const APPLICATION: &str = r"spec:
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

const CHARTS: &str = "https://codecentric.github.io/helm-charts";

#[test]
fn only_the_chart_source_moves() {
    let out = retarget(APPLICATION, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    assert!(out.contains("targetRevision: 7.3.1"));
    assert!(
        out.contains("targetRevision: PLACEHOLDER"),
        "the platform repository's own source is not a chart and must not move:\n{out}"
    );
}

#[test]
fn the_comment_above_the_pin_survives() {
    // These files are written by people and their comments say why a pin is
    // what it is. A load-and-dump would delete them and attach the damage to a
    // routine version bump.
    let out = retarget(APPLICATION, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    assert!(out.contains("# The upstream chart. Bumping this is a deliberate act."));
    assert!(out.contains("releaseName: keycloak"), "{out}");
}

#[test]
fn a_chart_from_another_repository_is_not_this_chart() {
    // The reason both halves of the identity are matched. A source naming
    // `keycloakx` from somewhere else is different software, and pinning it
    // because the names agree is how the wrong thing gets deployed.
    let failure = retarget(APPLICATION, "https://charts.example.test", "keycloakx", "7.3.1")
        .expect_err("the repository has to match too");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[test]
fn a_file_naming_no_such_chart_is_refused_rather_than_left_alone() {
    // Silence would be worse than a refusal: the manifest would record a
    // version the cluster never received, and nothing would say so.
    let failure = retarget(APPLICATION, CHARTS, "postgresql", "1.0.0").expect_err("no source names it");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}

#[test]
fn an_ambiguous_file_is_refused_rather_than_guessed_between() {
    let twice = APPLICATION.replace(
        "    - repoURL: https://github.com/FieldstateNZ/saas-fabric-platform.git\n      targetRevision: PLACEHOLDER\n      ref: platform\n",
        "    - repoURL: https://codecentric.github.io/helm-charts\n      chart: keycloakx\n      targetRevision: 7.2.0\n",
    );

    let failure = retarget(&twice, CHARTS, "keycloakx", "7.3.1")
        .expect_err("two sources naming one chart is not a file to edit");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}
