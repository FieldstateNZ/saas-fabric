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

#[test]
fn a_lookalike_list_elsewhere_in_the_file_is_not_a_source() {
    // The renderer knows one location, not one shape. A list of things that
    // happen to carry `chart:` and `targetRevision:` outside `spec.sources`
    // is somebody else's data, and editing it because it looks similar is the
    // arbitrary-edit engine this design exists to refuse.
    let decoy = format!(
        "metadata:
  annotations:
    inventory: |
      - repoURL: {CHARTS}
        chart: keycloakx
        targetRevision: 0.0.1
{APPLICATION}"
    );

    let out = retarget(&decoy, CHARTS, "keycloakx", "7.3.1").expect("the real source still matches");

    assert!(
        out.contains("targetRevision: 0.0.1"),
        "the annotation is not a source and must be untouched:\n{out}"
    );
    assert!(out.contains("targetRevision: 7.3.1"));
}

#[test]
fn a_source_of_a_source_is_not_a_source() {
    // Nested lists inside a source -- values files, parameters -- are not
    // entries of `spec.sources` and must not be read as if they were.
    let nested = APPLICATION.replace(
        "      helm:\n        releaseName: keycloak\n",
        "      helm:\n        parameters:\n          - name: image.tag\n            value: 26.7.2\n",
    );

    let out = retarget(&nested, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    assert!(out.contains("value: 26.7.2"), "{out}");
    assert!(out.contains("targetRevision: 7.3.1"));
}

#[test]
fn a_release_from_another_chart_is_not_written_into_this_pin() {
    // The rule the renderer alone cannot enforce, because by the time it is
    // called somebody has already decided which file and which version. What
    // it *can* enforce is that the chart the pin names is the chart it edits —
    // and `render` above it checks that the artifact, the release and the pin
    // all name the same one, because a version is only a number and a number
    // is plausible against the wrong chart.
    let failure = retarget(APPLICATION, CHARTS, "postgresql", "7.3.1")
        .expect_err("this file pins keycloakx and nothing else");

    assert!(
        matches!(failure, PlatformGitError::Rejected { .. }),
        "{failure:?}"
    );
}
