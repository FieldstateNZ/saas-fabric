//! Which Argo source a chart pin is allowed to move, and what it may rewrite.

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

/// Asserts the rewrite changed exactly one line, and changed nothing in it but
/// the version.
///
/// `contains` proves a version arrived somewhere; it does not prove the file is
/// otherwise the file that went in. Every byte matters here — a diff that says
/// more than "one version moved" is a diff nobody reviews carefully — so the
/// success tests compare the two files line for line instead.
fn only_the_revision_moved(before: &str, after: &str, was: &str, now: &str) {
    let mut olds = before.split_inclusive('\n');
    let mut news = after.split_inclusive('\n');
    let mut changed = 0_usize;

    loop {
        match (olds.next(), news.next()) {
            (None, None) => break,
            (old, new) if old == new => {}
            (Some(old), Some(new)) => {
                changed += 1;
                assert_eq!(
                    old.replacen(was, now, 1),
                    new,
                    "the line changed by more than its version"
                );
            }
            (old, new) => panic!("the file gained or lost a line: {old:?} became {new:?}"),
        }
    }

    assert_eq!(changed, 1, "exactly one line may change:\n{after}");
}

/// The refusal's detail, or a panic naming what came back instead.
fn refusal(outcome: Result<String, PlatformGitError>) -> String {
    match outcome {
        Err(PlatformGitError::Rejected { detail }) => detail,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// A source with `shape` written where its `targetRevision` value belongs.
fn with_revision(shape: &str) -> String {
    format!(
        "spec:
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      targetRevision:{shape}
"
    )
}

#[test]
fn only_the_chart_source_moves() {
    let out = retarget(APPLICATION, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(APPLICATION, &out, "7.3.0", "7.3.1");
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
    refusal(retarget(
        APPLICATION,
        "https://charts.example.test",
        "keycloakx",
        "7.3.1",
    ));
}

#[test]
fn a_file_naming_no_such_chart_is_refused_rather_than_left_alone() {
    // Silence would be worse than a refusal: the manifest would record a
    // version the cluster never received, and nothing would say so.
    refusal(retarget(APPLICATION, CHARTS, "postgresql", "1.0.0"));
}

#[test]
fn an_ambiguous_file_is_refused_rather_than_guessed_between() {
    let twice = APPLICATION.replace(
        "    - repoURL: https://github.com/FieldstateNZ/saas-fabric-platform.git\n      targetRevision: PLACEHOLDER\n      ref: platform\n",
        "    - repoURL: https://codecentric.github.io/helm-charts\n      chart: keycloakx\n      targetRevision: 7.2.0\n",
    );

    let detail = refusal(retarget(&twice, CHARTS, "keycloakx", "7.3.1"));

    assert!(detail.contains('2'), "the refusal says how many: {detail}");
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

    only_the_revision_moved(&decoy, &out, "7.3.0", "7.3.1");
    assert!(
        out.contains("targetRevision: 0.0.1"),
        "the annotation is not a source and must be untouched:\n{out}"
    );
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

    only_the_revision_moved(&nested, &out, "7.3.0", "7.3.1");
    assert!(out.contains("value: 26.7.2"), "{out}");
}

#[test]
fn a_release_from_another_chart_is_not_written_into_this_pin() {
    // The rule the renderer alone cannot enforce, because by the time it is
    // called somebody has already decided which file and which version. What
    // it *can* enforce is that the chart the pin names is the chart it edits —
    // and `render` above it checks that the artifact, the release and the pin
    // all name the same one, because a version is only a number and a number
    // is plausible against the wrong chart.
    refusal(retarget(APPLICATION, CHARTS, "postgresql", "7.3.1"));
}

#[test]
fn a_sources_list_inside_a_source_is_not_the_sources_list() {
    // Without this, a `sources:` nested in a source's own `helm:` block is
    // entered as if it were `spec.sources`, and its entries become candidates
    // -- so a values block that mentions the same chart makes the file look
    // ambiguous, or gets edited instead of the real pin.
    let decoy = APPLICATION.replace(
        "      helm:\n        releaseName: keycloak\n",
        &format!(
            "      helm:\n        sources:\n          - repoURL: {CHARTS}\n            chart: keycloakx\n            targetRevision: 0.0.1\n"
        ),
    );

    let out = retarget(&decoy, CHARTS, "keycloakx", "7.3.1").expect("only the real source matches");

    only_the_revision_moved(&decoy, &out, "7.3.0", "7.3.1");
    assert!(out.contains("targetRevision: 0.0.1"), "{out}");
}

#[test]
fn a_sources_list_under_another_spec_key_is_not_spec_sources() {
    // `spec.sources` means the direct child of `spec:`. A `sources:` under
    // `spec.template` belongs to something else, and reading it as the list
    // means the real one is never reached.
    let decoy = format!(
        "spec:
  template:
    sources:
      - repoURL: {CHARTS}
        chart: keycloakx
        targetRevision: 0.0.1
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      targetRevision: 7.3.0
"
    );

    let out = retarget(&decoy, CHARTS, "keycloakx", "7.3.1").expect("the direct list is the list");

    only_the_revision_moved(&decoy, &out, "7.3.0", "7.3.1");
    assert!(out.contains("targetRevision: 0.0.1"), "{out}");
}

#[test]
fn a_file_whose_only_sources_list_is_nested_names_no_source() {
    // The same rule with nothing to fall back on: a file that has only the
    // decoy has no source to move, and saying so is better than moving the
    // decoy.
    let only_decoy = format!(
        "spec:
  template:
    sources:
      - repoURL: {CHARTS}
        chart: keycloakx
        targetRevision: 0.0.1
"
    );

    refusal(retarget(&only_decoy, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn a_target_revision_nested_deeper_in_the_source_is_not_the_one_edited() {
    // A source's own `targetRevision` is a direct key of it. One further in --
    // a Helm parameter, a values block -- is data the chart consumes, and
    // rewriting it moves something nobody asked to move.
    let deeper = APPLICATION.replace(
        "        releaseName: keycloak\n",
        "        releaseName: keycloak\n        parameters:\n          targetRevision: 0.0.1\n",
    );

    let out = retarget(&deeper, CHARTS, "keycloakx", "7.3.1").expect("the direct key is the pin");

    only_the_revision_moved(&deeper, &out, "7.3.0", "7.3.1");
    assert!(out.contains("targetRevision: 0.0.1"), "{out}");
}

#[test]
fn a_source_with_no_target_revision_of_its_own_is_refused() {
    // With only a nested one to find, the answer is a refusal rather than the
    // nested one.
    let buried = format!(
        "spec:
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      helm:
        values:
          targetRevision: 0.0.1
"
    );

    let detail = refusal(retarget(&buried, CHARTS, "keycloakx", "7.3.1"));

    assert!(detail.contains("targetRevision"), "{detail}");
}

#[test]
fn a_source_that_says_its_revision_twice_is_refused() {
    let twice = APPLICATION.replace(
        "      targetRevision: 7.3.0\n",
        "      targetRevision: 7.3.0\n      targetRevision: 7.3.0\n",
    );

    refusal(retarget(&twice, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn a_source_that_says_its_repository_twice_is_refused() {
    // Which repository the source names depends on which line is read, so
    // whether it matches at all cannot be told.
    let twice = APPLICATION.replace(
        "    - repoURL: https://codecentric.github.io/helm-charts\n",
        "    - repoURL: https://codecentric.github.io/helm-charts\n      repoURL: https://charts.example.test\n",
    );

    refusal(retarget(&twice, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn a_source_that_says_its_chart_twice_is_refused() {
    let twice = APPLICATION.replace(
        "      chart: keycloakx\n",
        "      chart: keycloakx\n      chart: postgresql\n",
    );

    refusal(retarget(&twice, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn the_order_of_a_sources_keys_does_not_matter() {
    // A mapping's keys have no order in YAML. Reading the file once and
    // deciding afterwards is what makes that true here too -- a walk that
    // edits as it goes never sees the `repoURL` that would have told it this
    // was the source.
    let reordered = format!(
        "spec:
  sources:
    - targetRevision: 7.3.0
      chart: keycloakx
      repoURL: {CHARTS}
"
    );

    let out = retarget(&reordered, CHARTS, "keycloakx", "7.3.1").expect("order is not identity");

    only_the_revision_moved(&reordered, &out, "7.3.0", "7.3.1");
}

#[test]
fn a_block_scalar_opening_a_source_hides_none_of_that_sources_other_keys() {
    // `ref` is an Argo source field and a folded scalar is a legal way to
    // write it. A block header measured at the dash's column rather than the
    // key's would read every other key of this source as the scalar's body,
    // and the source would look like it named no chart at all.
    let folded = format!(
        "spec:
  sources:
    - ref: >-
        values
      repoURL: {CHARTS}
      chart: keycloakx
      targetRevision: 7.3.0
"
    );

    let out = retarget(&folded, CHARTS, "keycloakx", "7.3.1").expect("the keys after the scalar are read");

    only_the_revision_moved(&folded, &out, "7.3.0", "7.3.1");
}

#[test]
fn a_block_scalar_opening_a_second_source_does_not_hide_the_ambiguity() {
    // The dangerous half of the case above. Two sources name this chart; if
    // the second were swallowed as a block body, the file would look
    // unambiguous and the first would be edited -- choosing between them by
    // accident, which is exactly how the wrong chart gets deployed.
    let hidden_twin = format!(
        "spec:
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      targetRevision: 7.3.0
    - ref: >-
        values
      repoURL: {CHARTS}
      chart: keycloakx
      targetRevision: 9.9.9
"
    );

    let detail = refusal(retarget(&hidden_twin, CHARTS, "keycloakx", "7.3.1"));

    assert!(detail.contains('2'), "both sources are seen: {detail}");
}

#[test]
fn a_key_that_merely_starts_with_the_revision_key_is_not_the_revision_key() {
    // `targetRevisionOverride` is a different key. Matching on a prefix would
    // make it this pin, and move something nobody named.
    let lookalike = format!(
        "spec:
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      targetRevisionOverride: 9.9.9
      targetRevision: 7.3.0
"
    );

    let out = retarget(&lookalike, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&lookalike, &out, "7.3.0", "7.3.1");
    assert!(out.contains("targetRevisionOverride: 9.9.9"), "{out}");
}

#[test]
fn a_shape_this_cannot_read_is_refused_rather_than_guessed_at() {
    // Every one of these has a reading, and every reading is a guess. A
    // renderer that guesses at a file it does not understand writes a file
    // nobody predicted -- which is exactly what a GitOps repository must never
    // receive from a machine.
    let cases: [(&str, String); 11] = [
        (
            "a flow-style sources list",
            format!("spec:\n  sources: [{{repoURL: {CHARTS}, chart: keycloakx, targetRevision: 7.3.0}}]\n"),
        ),
        (
            "a spec written on one line",
            format!("spec: {{sources: [{{repoURL: {CHARTS}, chart: keycloakx}}]}}\n"),
        ),
        ("a revision with no value", with_revision("")),
        ("a block scalar", with_revision(" |\n        7.3.0")),
        ("a folded scalar", with_revision(" >\n        7.3.0")),
        ("a flow sequence", with_revision(" [7.3.0]")),
        ("a flow mapping", with_revision(" {a: 1}")),
        ("an anchor", with_revision(" &rev 7.3.0")),
        ("an alias", with_revision(" *rev")),
        ("a tag", with_revision(" !!str 7.3.0")),
        (
            "a tab in the indentation",
            format!("spec:\n  sources:\n    - repoURL: {CHARTS}\n\t  chart: keycloakx\n      targetRevision: 7.3.0\n"),
        ),
    ];

    for (label, text) in cases {
        let outcome = retarget(&text, CHARTS, "keycloakx", "7.3.1");
        assert!(
            matches!(outcome, Err(PlatformGitError::Rejected { .. })),
            "{label} must be refused, got {outcome:?}"
        );
    }
}

#[test]
fn a_quoted_revision_that_escapes_anything_is_refused() {
    // Reading an escape means implementing YAML's, and writing the value back
    // means implementing it in reverse. Neither belongs in a version bump.
    for shape in [" \"7\\\"3\"", " '7''3'"] {
        let text = with_revision(shape);
        let outcome = retarget(&text, CHARTS, "keycloakx", "7.3.1");
        assert!(
            matches!(outcome, Err(PlatformGitError::Rejected { .. })),
            "{shape} must be refused, got {outcome:?}"
        );
    }
}

#[test]
fn a_quoted_revision_keeps_its_quotes() {
    // The quoting is the author's, not this renderer's. Dropping it is a
    // change to a line that was only supposed to gain a version.
    for quote in ['"', '\''] {
        let text = with_revision(&format!(" {quote}7.3.0{quote}"));
        let out = retarget(&text, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

        only_the_revision_moved(&text, &out, "7.3.0", "7.3.1");
        assert!(out.contains(&format!("{quote}7.3.1{quote}")), "{out}");
    }
}

#[test]
fn a_comment_after_the_revision_survives_byte_for_byte() {
    // The comment says why the pin is what it is, and it sits on the one line
    // this renderer rewrites -- so it is the comment most easily lost.
    let noted = APPLICATION.replace(
        "      targetRevision: 7.3.0\n",
        "      targetRevision: 7.3.0   # held here until the CVE lands\n",
    );

    let out = retarget(&noted, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&noted, &out, "7.3.0", "7.3.1");
    assert!(
        out.contains("targetRevision: 7.3.1   # held here until the CVE lands"),
        "{out}"
    );
}

#[test]
fn a_file_written_with_crlf_stays_written_with_crlf() {
    // A repository checked out on Windows, or one with a `.gitattributes` that
    // says so. Rewriting every terminator turns a one-line bump into a
    // whole-file diff, and hides the change nobody would otherwise have missed.
    let crlf = APPLICATION.replace('\n', "\r\n");

    let out = retarget(&crlf, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&crlf, &out, "7.3.0", "7.3.1");
    assert_eq!(
        out.matches("\r\n").count(),
        crlf.matches("\r\n").count(),
        "{out:?}"
    );
}

#[test]
fn each_line_keeps_the_terminator_it_had() {
    // A file that is already mixed is not this renderer's to tidy.
    let mixed = APPLICATION.replacen("  sources:\n", "  sources:\r\n", 1);

    let out = retarget(&mixed, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&mixed, &out, "7.3.0", "7.3.1");
    assert_eq!(out.matches("\r\n").count(), 1, "{out:?}");
}

#[test]
fn a_file_with_no_final_newline_does_not_gain_one() {
    let unterminated = APPLICATION.trim_end_matches('\n');

    let out = retarget(unterminated, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(unterminated, &out, "7.3.0", "7.3.1");
    assert!(!out.ends_with('\n'), "{out:?}");
}

#[test]
fn a_file_with_a_final_newline_keeps_it() {
    let out = retarget(APPLICATION, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    assert!(out.ends_with("ref: platform\n"), "{out:?}");
}

#[test]
fn a_revision_on_the_dash_line_is_still_this_sources_revision() {
    // `- targetRevision: 7.3.0` is the entry's first key written where YAML
    // allows it to be written. Rewriting it must keep the `- ` in front.
    let inline = format!(
        "spec:
  sources:
    - targetRevision: 7.3.0
      repoURL: {CHARTS}
      chart: keycloakx
"
    );

    let out = retarget(&inline, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&inline, &out, "7.3.0", "7.3.1");
    assert!(out.contains("    - targetRevision: 7.3.1"), "{out}");
}

#[test]
fn a_sequence_at_its_keys_own_indent_is_still_the_sources_list() {
    // Both indentations are the same YAML. Reading only the deeper one would
    // refuse a file Argo accepts.
    let flush = format!(
        "spec:
  sources:
  - repoURL: {CHARTS}
    chart: keycloakx
    targetRevision: 7.3.0
"
    );

    let out = retarget(&flush, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&flush, &out, "7.3.0", "7.3.1");
}

#[test]
fn a_second_document_after_the_marker_is_walked_too() {
    // A `---` starts a new document with its own `spec:`. Stopping at the
    // first one would leave the pin in the second unreachable.
    let two = format!(
        "spec:
  sources:
    - repoURL: https://charts.example.test
      chart: postgresql
      targetRevision: 1.0.0
---
{APPLICATION}"
    );

    let out = retarget(&two, CHARTS, "keycloakx", "7.3.1").expect("the second document matches");

    only_the_revision_moved(&two, &out, "7.3.0", "7.3.1");
    assert!(out.contains("targetRevision: 1.0.0"), "{out}");
}

#[test]
fn one_match_per_document_is_still_two_matches() {
    // "Exactly one" is a rule about the file, not about each document in it.
    let two = format!("{APPLICATION}---\n{APPLICATION}");

    refusal(retarget(&two, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn a_comment_a_tab_away_from_the_revision_survives_byte_for_byte() {
    // YAML separates a comment from a value with a run of spaces *or tabs*.
    // Looking only for `" #"` reads the tab and the comment as part of the
    // version, and replacing the value then deletes the whole comment -- a
    // silent loss on the success path, on the one line this renderer rewrites.
    let noted = APPLICATION.replace(
        "      targetRevision: 7.3.0\n",
        "      targetRevision: 7.3.0\t# held here until the CVE lands\n",
    );

    let out = retarget(&noted, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&noted, &out, "7.3.0", "7.3.1");
    assert!(
        out.contains("targetRevision: 7.3.1\t# held here until the CVE lands"),
        "{out:?}"
    );
}

#[test]
fn a_comment_a_tab_away_from_the_repository_still_names_the_source() {
    // The same blind spot on an identity key does not lose bytes -- it loses
    // the match, and reports a file that plainly names the chart as naming no
    // source at all.
    let noted = APPLICATION.replace(
        "    - repoURL: https://codecentric.github.io/helm-charts\n",
        "    - repoURL: https://codecentric.github.io/helm-charts\t# upstream\n",
    );

    let out = retarget(&noted, CHARTS, "keycloakx", "7.3.1").expect("the comment is not the URL");

    only_the_revision_moved(&noted, &out, "7.3.0", "7.3.1");
}

#[test]
fn a_tab_inside_a_values_block_is_content_rather_than_indentation() {
    // A pasted script or JSON blob under `helm.values: |` is a block scalar's
    // text, and a tab in it is a byte of that text -- legal YAML, which Argo
    // loads. Refusing the whole promotion for it stops a bump over something
    // that is none of this renderer's business.
    let values = APPLICATION.replace(
        "        releaseName: keycloak\n",
        "        releaseName: keycloak\n        values: |\n          {\n          \t\"replicas\": 1\n          }\n",
    );

    let out = retarget(&values, CHARTS, "keycloakx", "7.3.1").expect("the tab is content");

    only_the_revision_moved(&values, &out, "7.3.0", "7.3.1");
    assert!(out.contains("\t\"replicas\": 1"), "{out:?}");
}

#[test]
fn a_revision_inside_a_values_block_is_not_this_sources_revision() {
    // The body of a block scalar is text. A line in it that reads like a key
    // is not one, and the walk must not observe it as structure.
    let values = APPLICATION.replace(
        "        releaseName: keycloak\n",
        "        releaseName: keycloak\n        values: |\n          targetRevision: 0.0.1\n",
    );

    let out = retarget(&values, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&values, &out, "7.3.0", "7.3.1");
    assert!(out.contains("targetRevision: 0.0.1"), "{out}");
}

#[test]
fn a_key_written_without_separation_after_the_colon_is_not_a_key() {
    // `targetRevision:7.3.0` is one plain scalar in YAML, not a mapping key.
    // Reading it as a key is how a renderer starts editing text that means
    // something else -- and the reason the colon alone is not enough to match.
    let jammed = format!(
        "spec:
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      targetRevision:7.3.0
"
    );

    refusal(retarget(&jammed, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn a_spec_below_the_documents_top_level_is_not_the_spec() {
    // `spec.sources` means a `spec:` at column 0. Inside a `kind: List`
    // wrapper every Application's `spec:` is indented, and reading one of them
    // would edit an Application this pin never named.
    let wrapped = format!(
        "kind: List
items:
  - apiVersion: argoproj.io/v1alpha1
    spec:
      sources:
        - repoURL: {CHARTS}
          chart: keycloakx
          targetRevision: 7.3.0
"
    );

    refusal(retarget(&wrapped, CHARTS, "keycloakx", "7.3.1"));
}

#[test]
fn a_file_ending_on_the_pinned_line_does_not_gain_a_newline() {
    // The rewritten line is the one whose terminator is easiest to invent,
    // because it is the only line this renderer builds rather than copies.
    let ends_on_the_pin = format!(
        "spec:
  sources:
    - repoURL: {CHARTS}
      chart: keycloakx
      targetRevision: 7.3.0"
    );

    let out = retarget(&ends_on_the_pin, CHARTS, "keycloakx", "7.3.1").expect("one source matches");

    only_the_revision_moved(&ends_on_the_pin, &out, "7.3.0", "7.3.1");
    assert!(out.ends_with("targetRevision: 7.3.1"), "{out:?}");
}

#[test]
fn a_version_yaml_would_not_read_back_is_refused() {
    // The renderer is handed a version by discovery, and the caller above it
    // takes an unvalidated `String`. One carrying a comment marker, a quote or
    // a colon would rewrite the line into something that is not YAML, which is
    // the one outcome byte preservation exists to prevent. What a chart
    // repository publishes is SemVer's alphabet and nothing else.
    for version in [
        "",
        "7.3.1 # oops",
        "7.3.1\"",
        "*7.3.1",
        "7.3.1:",
        "]7.3.1",
        "7.3.1\\u",
    ] {
        let outcome = retarget(APPLICATION, CHARTS, "keycloakx", version);
        assert!(
            matches!(outcome, Err(PlatformGitError::Rejected { .. })),
            "{version:?} must be refused, got {outcome:?}"
        );
    }
}

#[test]
fn an_empty_file_names_no_source() {
    refusal(retarget("", CHARTS, "keycloakx", "7.3.1"));
}
