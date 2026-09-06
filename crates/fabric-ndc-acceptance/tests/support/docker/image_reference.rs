//! The pinned-image policy: what `docker run` should actually pull, and what
//! it means for an image to already be "present" under a digest pin.
//!
//! Split out of `containers.rs` (`docs/architecture/file-size-policy.md`'s
//! "one concept per file" convention, which reviewers hold test support to
//! as well): this is a self-contained policy -- check whether the pin is
//! already present before ever pulling, pull it (with a deadline) only on
//! absence, and only fall back to an unpinned tag outside the required mode
//! when that pull itself fails -- worth reading and reviewing on its own
//! rather than folded into container start/stop/exec plumbing.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use crate::support::gate;

use super::process::{self, DockerError};

/// How long [`resolve_runnable_reference`] gives a pull before giving up.
///
/// CI's `connector-acceptance` job pre-pulls every pinned image as its own
/// step before the test binary ever runs (`.github/workflows/ci.yml`), so
/// the pull this function attempts is a same-image cache hit there and
/// returns almost immediately -- this deadline is never the thing CI waits
/// on. On a machine whose Docker registry route is blocked (this repository
/// has one -- see [`super::super::images::NDC_POSTGRES`]'s doc comment),
/// there is no way to learn that *without* attempting the pull, and two
/// minutes is the cost of learning it -- paid once per process, not once per
/// test, because [`resolve_runnable_reference`] caches the outcome.
const PULL_DEADLINE: Duration = Duration::from_secs(120);

/// Reduces `reference` to the one string this module inspects, pulls, and
/// hands to `docker run`: `repository@sha256:...` when `reference` carries a
/// digest, or `reference` unchanged when it does not.
///
/// Docker resolves a `name:tag@digest` reference by its digest alone -- the
/// tag half is never load-bearing to `pull`, `run`, or `image inspect` -- and
/// a pulled image's own `RepoDigests` entry never carries a tag either, only
/// `name@digest`. Inspecting the combined `name:tag@digest` form can
/// therefore report an image absent that is genuinely present under its
/// digest. Routing every digest-qualified reference through this one
/// reduction before it is inspected, pulled, or returned is what makes "the
/// image proven present is the image that runs" a property of this code,
/// rather than a fact that merely happens to be true of Docker's reference
/// grammar today.
///
/// A colon is stripped only from the final `/`-separated segment: the
/// repository name can itself carry a registry host with a port
/// (`localhost:5000/foo`), and stripping after the *last* colon anywhere in
/// the string would truncate the registry host instead of the tag.
///
/// `pub(super)`, not private: its unit tests live in the sibling
/// `image_reference_tests.rs`, declared alongside this module in
/// `docker.rs` rather than nested inside it (this repository's `*_tests.rs`
/// convention -- see `crates/fabric-connector-ndc/src/translate/mutation_tests.rs`
/// for another crate's version of the same pattern), so they sit outside
/// this module and need at least `docker`-and-below visibility to reach it.
pub(super) fn repository_digest_form(reference: &str) -> String {
    let Some((name_and_tag, digest)) = reference.split_once('@') else {
        return reference.to_owned();
    };

    let last_segment_start = name_and_tag.rfind('/').map_or(0, |index| index + 1);
    let (prefix, last_segment) = name_and_tag.split_at(last_segment_start);
    let last_segment = last_segment
        .rsplit_once(':')
        .map_or(last_segment, |(repo, _tag)| repo);

    format!("{prefix}{last_segment}@{digest}")
}

/// Whether the reference `inspected` -- already reduced to a bare tag or a
/// [`repository_digest_form`] -- is present locally.
///
/// Takes the already-reduced string rather than reducing a raw reference
/// itself: its only caller, [`resolve_runnable_reference_uncached`], has
/// already made the one [`repository_digest_form`] call the pull path needs,
/// and asking this function to redo that reduction would risk it computing
/// a second, merely-equal string instead of reusing the first. That sharing
/// -- not merely both values happening to be equal -- is what makes "the
/// image inspected is the image pulled is the image returned" true by
/// construction: there is exactly one call to [`repository_digest_form`]
/// per reference on the pull path, and every use of its result is a clone
/// of that one `String`.
///
/// Never attempts a pull itself: [`resolve_runnable_reference`] owns when a
/// pull happens, so this stays a pure presence check.
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started.
fn image_present_at(inspected: &str) -> Result<bool, DockerError> {
    let output = process::spawn(&["image".to_owned(), "inspect".to_owned(), inspected.to_owned()])?;
    Ok(output.status.success())
}

/// One reference's resolution outcome, computed at most once. A cell is
/// created empty and filled by whichever caller reaches
/// [`OnceLock::get_or_init`] first; every other caller for the same
/// reference -- including one racing it on another thread -- blocks on that
/// same call instead of starting a second one, per [`OnceLock`]'s own
/// contract.
type ResolutionCell = Arc<OnceLock<Result<String, String>>>;

/// One [`ResolutionCell`] per distinct reference string, for the rest of
/// this process's life.
///
/// A reference's resolution cannot change within a process: `docker`'s
/// answer to "is this pin present" and "did the pull succeed" depends on the
/// local daemon and network, neither of which this harness expects to
/// change mid-run. `cargo test` runs a binary's tests concurrently by
/// default, and this crate's (currently thirteen -- ten in
/// `published_state_reaches_a_real_connector.rs`, three in
/// `the_stack_comes_up.rs`) `Stack`-backed tests each call
/// [`super::containers::run`] for the connector image, so several can reach
/// this function for the same reference before any of them has finished
/// resolving it. (A fourteenth container-backed test,
/// `a_connector_that_answers_http_but_not_ndc_is_refused_rather_than_believed`,
/// starts only the `Impostor`'s nginx and never asks this function for the
/// connector image at all.) Keying the map by reference and putting a
/// [`OnceLock`] behind each key -- rather than caching a plain
/// `Result<String, String>` after the fact -- is what makes that race safe:
/// the second and every later caller for a reference, whether it arrives a
/// millisecond later on another thread or an hour later in another test,
/// blocks on the *one* in-flight resolution instead of starting its own.
/// Without this, a required-mode run on a machine that cannot pull the pin
/// could pay [`PULL_DEADLINE`] several times over -- once per thread that
/// raced to be first -- instead of exactly once. Failures are cached too,
/// deliberately: a resolution that fails once fails the same way every
/// later time in this process, and re-attempting it would just pay the
/// deadline again for the same answer.
static RESOLVED: LazyLock<Mutex<BTreeMap<String, ResolutionCell>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Resolves `reference` to the string [`containers::run`](super::containers::run)
/// should actually pass to `docker run`, via [`RESOLVED`] so the real work in
/// [`resolve_runnable_reference_uncached`] runs at most once per reference
/// per process -- see that static's own doc for why a plain post-hoc cache
/// would not be enough under `cargo test`'s default concurrency.
///
/// A poisoned [`RESOLVED`] mutex (a prior panic while the map itself was
/// being locked, which nothing here ever holds for longer than a map
/// lookup or insert) is treated as merely contended rather than propagated:
/// the map's own contents are never left inconsistent by a panic elsewhere,
/// since the expensive work happens inside the per-reference `OnceLock`
/// after this lock is already released.
///
/// # Errors
///
/// See [`resolve_runnable_reference_uncached`]. A cached failure is
/// rewrapped as a fresh [`DockerError`] carrying the same message.
pub(super) fn resolve_runnable_reference(reference: &str) -> Result<String, DockerError> {
    let cell: ResolutionCell = RESOLVED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(reference.to_owned())
        .or_default()
        .clone();

    cell.get_or_init(|| resolve_runnable_reference_uncached(reference).map_err(|error| error.to_string()))
        .clone()
        .map_err(|message| DockerError::from_parts(format!("resolve {reference}"), message))
}

/// Resolves `reference` to the string `docker run` should be given, doing
/// the actual presence check and pull. Only ever called once per distinct
/// `reference` per process -- see [`resolve_runnable_reference`], its only
/// caller.
///
/// Checks locally first ([`image_present_at`]): most runs already have the
/// pin, and a pull is not free even when bounded. Only when it is absent
/// does this pull [`repository_digest_form`]'s reduction of `reference` --
/// the same string [`image_present_at`] inspected, and the same string this
/// returns on success.
///
/// # Absence always attempts a pull, digest or no digest
///
/// Earlier, a reference with no digest (a bare tag) skipped this function's
/// pull entirely when absent, returning the tag unchanged and leaving
/// `docker run` to trigger Docker's own implicit pull -- unbounded, and
/// exactly the hang this module exists to prevent. There is no such
/// reference in [`images`](crate::support::images) today:
/// [`NDC_POSTGRES`](crate::support::images::NDC_POSTGRES),
/// [`POSTGRES`](crate::support::images::POSTGRES), and
/// [`NGINX`](crate::support::images::NGINX) are all digest-qualified. The
/// uniform rule below -- absent means this function pulls, regardless of
/// whether `reference` carries a digest -- is future-proofing against a pin
/// that someday does not, not a behaviour change for any image this crate
/// runs now.
///
/// Only once the pull itself fails does this fall back at all, and only for
/// a digest-qualified `reference`: in the required mode, to an outright
/// error naming the digest and the pull's own failure; otherwise, to the
/// bare tag, if that is present locally -- the situation this repository is
/// in on at least one development machine (see
/// [`super::super::images::NDC_POSTGRES`]'s doc comment): a Docker Desktop
/// daemon whose registry route is blocked, with the tag loaded by hand under
/// a different digest than the pinned index. That fallback is announced
/// with an `eprintln!`, but the announcement is not itself the guarantee
/// anyone will see it: like `gate.rs`'s skip line, it appears only under
/// `--nocapture` or on failure, because libtest captures a passing test's
/// stderr otherwise and prints none of it in the ordinary summary.
/// [`gate::REQUIRE_ENV`] is the guarantee that governs whether the fallback
/// can happen at all, not this line. A bare tag with no digest has no such
/// fallback to offer -- there is no second reference to try -- so its pull
/// failure is returned as-is.
///
/// If neither the pull nor the bare-tag fallback succeeds, this returns the
/// pull's own error rather than pressing on to a `docker run` that would
/// only fail again, less specifically: the pull failure already names the
/// digest and the registry's own complaint about it.
///
/// # The required mode disables the fallback
///
/// [`gate::REQUIRE_ENV`] set to `1` -- which CI's `connector-acceptance` job
/// always does -- turns a failed pull of a digest-qualified image into an
/// outright failure naming the digest and the pull's `stderr`, never a
/// silent fallback to the bare tag. See `gate.rs`'s module doc for why: the
/// fallback exists for exactly one situation, a sandboxed developer machine
/// whose daemon cannot pull. CI's daemon pulls normally, so a pull failure
/// there is not that situation -- it is drift between the pin and what the
/// registry now serves, or a typo in the pin -- and running whatever the
/// bare tag happens to resolve to would be running an unpinned image while a
/// pinned-looking test result reported success. Required mode is what makes
/// that impossible instead of merely unlikely.
///
/// # Errors
///
/// A [`DockerError`] if `docker` itself could not be started; if the pull
/// did not complete within [`PULL_DEADLINE`]; if [`gate::REQUIRE_ENV`] is
/// `1` and `reference`'s pinned digest could not be pulled; or, outside the
/// required mode, if the pull failed and (for a digest-qualified reference)
/// the bare tag is not present locally either -- in which case this returns
/// the pull's own error, since a `docker run` failure moments later would
/// say less than the pull failure already does.
fn resolve_runnable_reference_uncached(reference: &str) -> Result<String, DockerError> {
    let digest_form = repository_digest_form(reference);

    if image_present_at(&digest_form)? {
        return Ok(digest_form);
    }

    let pull = process::run_with_deadline(&["pull".to_owned(), digest_form.clone()], PULL_DEADLINE);
    let Err(pull_error) = pull else {
        return Ok(digest_form);
    };

    // Only a digest-qualified reference has a bare-tag form to fall back to.
    let Some((name_and_tag, digest)) = reference.split_once('@') else {
        return Err(pull_error);
    };

    if std::env::var(gate::REQUIRE_ENV).as_deref() == Ok("1") {
        return Err(DockerError::required_digest_missing(
            &digest_form,
            digest,
            &pull_error,
        ));
    }

    if image_present_at(name_and_tag)? {
        eprintln!(
            "fabric-ndc-acceptance: {digest_form} could not be pulled within {PULL_DEADLINE:?} \
             ({pull_error}); falling back to the bare tag {name_and_tag} (see images.rs for why)"
        );
        return Ok(name_and_tag.to_owned());
    }

    Err(pull_error)
}

impl DockerError {
    /// Built by [`resolve_runnable_reference_uncached`] when the required
    /// mode refuses the bare-tag fallback after the pull itself failed.
    /// Carries the pull's own failure detail (command line, deadline, and
    /// `stderr`) alongside the digest, so the caller sees both what was
    /// missing and why fetching it did not fix that.
    fn required_digest_missing(reference: &str, digest: &str, pull_error: &DockerError) -> Self {
        Self::from_parts(
            format!("docker pull {reference}"),
            format!(
                "digest {digest} is not present locally and the pull failed ({pull_error}), and \
                 {}=1 disables the bare-tag fallback -- pull it by hand, or update the pin in \
                 images.rs if the registry has moved on",
                gate::REQUIRE_ENV
            ),
        )
    }
}
