//! Unit tests for [`image_reference`](super::image_reference), kept as a
//! sibling file rather than nested inside it -- this repository's
//! `*_tests.rs` convention for a module whose tests would otherwise push it
//! well past a size a reviewer should have to hold in mind as one concept
//! (see `crates/fabric-connector-ndc/src/translate/mutation_tests.rs` for
//! another crate's version of the same pattern). Declared alongside
//! `image_reference` in `docker.rs`, not inside `image_reference.rs` itself:
//! a file-based module's own submodules resolve under a directory named
//! after it, so a true sibling file has to be declared by the *parent*
//! module instead, exactly the way `translate.rs` declares both `mutation`
//! and `mutation_tests` side by side.
//!
//! Being a sibling rather than a nested `mod tests` costs
//! [`repository_digest_form`](super::image_reference::repository_digest_form)
//! its previous crate-private visibility: a sibling module cannot see
//! another module's private items, only `pub(super)` (or wider) ones, so
//! that function is `pub(super)` now -- visible within `docker` and below,
//! which is exactly this file's own reach, and no wider.

use super::image_reference::repository_digest_form;

#[test]
fn a_digest_qualified_reference_drops_its_tag() {
    assert_eq!(
        repository_digest_form("ghcr.io/hasura/ndc-postgres:v3.1.0@sha256:abc"),
        "ghcr.io/hasura/ndc-postgres@sha256:abc"
    );
}

#[test]
fn a_bare_tag_is_returned_unchanged() {
    assert_eq!(repository_digest_form("postgres:16-alpine"), "postgres:16-alpine");
}

/// A registry host can itself carry a port (`localhost:5000/...`), so a
/// colon before the final `/`-separated segment is a registry port, not
/// a tag separator. Stripping after the last colon in the whole string
/// would truncate the registry host out of the repository name
/// entirely, leaving `localhost@sha256:x` instead of the correct
/// `localhost:5000/foo@sha256:x`.
#[test]
fn a_registry_port_is_not_mistaken_for_a_tag() {
    assert_eq!(
        repository_digest_form("localhost:5000/foo@sha256:x"),
        "localhost:5000/foo@sha256:x"
    );
}

/// Pins [`crate::support::images::NDC_POSTGRES`]'s own reduction against
/// a literal digest, so a hand-edit of that constant that forgets to
/// re-derive the digest half fails here instead of only showing up as a
/// pull of the wrong reference. This checks one crate constant's value,
/// not a property of [`repository_digest_form`] in general -- the three
/// tests above already cover the function's behaviour -- and it is
/// exactly the same reduction [`ci_pre_pulls_exactly_the_pinned_images`]
/// holds `.github/workflows/ci.yml`'s pre-pull step to.
#[test]
fn the_ndc_postgres_pin_reduces_to_its_digest_form() {
    assert_eq!(
        repository_digest_form(crate::support::images::NDC_POSTGRES),
        "ghcr.io/hasura/ndc-postgres@sha256:f91910ef5107aa80d31d82639e149b7f41f4a5bb3af9a369397d7d5965d79a57"
    );
}

/// The `docker pull ...` lines inside the `connector-acceptance` job's "Pull
/// the pinned connector-acceptance images" step, parsed from that one
/// step's own `run:` block rather than grepped from the whole file -- so a
/// `docker pull` line anywhere else in `ci.yml`, or a leftover line this
/// step no longer needs, cannot hide a mismatch behind a `contains` check.
///
/// Filters to lines that actually start with `docker pull` before counting
/// anything: the block is shell, not a list of pulls alone, so a `set -euo
/// pipefail` guard or a comment line sharing the block's indentation would
/// otherwise be counted as if it were a pull and blamed on a pin mismatch
/// that was never the real problem. Only whether a `docker pull` line is
/// present or missing should ever fail this check.
fn ci_pre_pull_lines(ci_yaml: &str) -> Vec<&str> {
    const STEP_NAME: &str = "- name: Pull the pinned connector-acceptance images";

    let step_start = ci_yaml
        .find(STEP_NAME)
        .unwrap_or_else(|| panic!("ci.yml no longer has a step named `{STEP_NAME}`"));
    let after_step = &ci_yaml[step_start..];

    let run_at = after_step
        .find("run: |")
        .unwrap_or_else(|| panic!("`{STEP_NAME}` no longer has a `run: |` block"));
    let run_line_start = after_step[..run_at].rfind('\n').map_or(0, |newline| newline + 1);
    let run_indent = run_at - run_line_start;

    let after_run_line = run_at + after_step[run_at..].find('\n').map_or(0, |offset| offset + 1);

    after_step[after_run_line..]
        .lines()
        .take_while(|line| {
            let indent = line.len() - line.trim_start().len();
            !line.trim().is_empty() && indent > run_indent
        })
        .map(str::trim)
        .filter(|line| line.starts_with("docker pull"))
        .collect()
}

/// `.github/workflows/ci.yml`'s `connector-acceptance` job pre-pulls
/// every pinned image by hand, precisely so a slow or failing pull shows
/// up in that step's own log rather than inside a test's timeout (see
/// that job's comment). Nothing regenerates that step from `images.rs`,
/// so this checks both directions: every pinned image has its
/// `docker pull` line (a bumped pin here with an un-bumped line there
/// would otherwise silently turn the pre-pull step into a no-op -- the
/// suite would still pass, since
/// [`resolve_runnable_reference`](super::image_reference::resolve_runnable_reference)
/// pulls again on the resulting cache miss, but the pull time -- now
/// bounded by `PULL_DEADLINE`, previously unbounded -- would move back inside
/// the job's `timeout-minutes`), and the step has *exactly* as many
/// pull lines as there are pinned images (a stale leftover line, for an
/// image no longer pinned, would otherwise pass silently forever).
#[test]
fn ci_pre_pulls_exactly_the_pinned_images() {
    let ci_yaml = include_str!("../../../../../.github/workflows/ci.yml");
    let pull_lines = ci_pre_pull_lines(ci_yaml);

    let images = [
        crate::support::images::NDC_POSTGRES,
        crate::support::images::POSTGRES,
        crate::support::images::NGINX,
    ];

    assert_eq!(
        pull_lines.len(),
        images.len(),
        "ci.yml's connector-acceptance pre-pull step has {} `docker pull` line(s) but images.rs \
         pins {} image(s) -- they must match one-for-one, or a stale or missing line would pass \
         silently. Lines found: {pull_lines:?}",
        pull_lines.len(),
        images.len()
    );

    for image in images {
        let expected = format!("docker pull {}", repository_digest_form(image));
        assert!(
            pull_lines.contains(&expected.as_str()),
            "ci.yml's connector-acceptance pre-pull step is missing `{expected}`"
        );
    }
}
