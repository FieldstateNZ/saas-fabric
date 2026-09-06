//! The pinned-image policy: what `docker run` should actually pull, and what
//! it means for an image to already be "present" under a digest pin.
//!
//! Split out of `containers.rs` (`docs/architecture/file-size-policy.md`'s
//! "one concept per file" convention, which reviewers hold test support to
//! as well): this is a self-contained policy -- pull the pin before giving
//! up on it, and only fall back to an unpinned tag outside the required mode
//! -- worth reading and reviewing on its own rather than folded into
//! container start/stop/exec plumbing.

use crate::support::gate;

use super::process::{self, DockerError};

/// Strips a trailing `:tag` from the repository half of a digest-qualified
/// reference, leaving `name@sha256:...` -- the unambiguous form
/// `docker image inspect` matches a local image's own `RepoDigests` against.
///
/// Docker's reference grammar accepts the combined `name:tag@digest` form
/// for `pull` and `run`, but a pulled image's `RepoDigests` entry never
/// carries the tag -- only `name@digest`. Inspecting by the combined form
/// can therefore report an image absent that is genuinely present under its
/// digest, which is why [`image_present`] normalises through here first.
///
/// `None` if `reference` carries no `@digest` suffix at all.
fn repository_digest_form(reference: &str) -> Option<String> {
    let (name_and_tag, digest) = reference.split_once('@')?;
    let repository = name_and_tag
        .rsplit_once(':')
        .map_or(name_and_tag, |(repo, _tag)| repo);
    Some(format!("{repository}@{digest}"))
}

/// Whether `reference` is present locally.
///
/// A digest-qualified reference is checked by its [`repository_digest_form`]
/// rather than the combined `name:tag@digest` string -- see that function's
/// doc for why the combined form can under-report. A bare tag (no `@`) is
/// inspected exactly as given.
///
/// Never attempts a pull itself: [`resolve_runnable_reference`] owns when a
/// pull happens, so this stays a pure presence check callable on its own.
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started.
pub fn image_present(reference: &str) -> Result<bool, DockerError> {
    let inspected = repository_digest_form(reference).unwrap_or_else(|| reference.to_owned());
    let output = process::spawn(&["image".to_owned(), "inspect".to_owned(), inspected])?;
    Ok(output.status.success())
}

/// Resolves `reference` to the string [`containers::run`](super::containers::run)
/// should actually pass to `docker run`.
///
/// Checks locally first ([`image_present`]): most runs already have the pin,
/// and a pull is not free. Only when it is absent does this pull `reference`
/// itself -- its full pinned digest form -- so a networked machine (CI
/// always is one) resolves precisely the pin, the same bytes `docker run`
/// would fetch on its own.
///
/// Only once that pull itself fails does this fall back at all: in the
/// required mode, to an outright error naming the digest and the pull's own
/// failure; otherwise, loudly, to the bare tag, if that is present locally --
/// the situation this repository is in on at least one development machine
/// (see [`images::NDC_POSTGRES`](super::super::images::NDC_POSTGRES)'s doc
/// comment): a sandboxed daemon that cannot pull, with the tag loaded by
/// hand under a different digest than the pinned index.
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
/// A [`DockerError`] only if `docker` itself could not be started, or if
/// [`gate::REQUIRE_ENV`] is `1` and `reference`'s pinned digest could not be
/// pulled.
pub(super) fn resolve_runnable_reference(reference: &str) -> Result<String, DockerError> {
    if image_present(reference)? {
        return Ok(reference.to_owned());
    }

    let Some((tag, digest)) = reference.split_once('@') else {
        return Ok(reference.to_owned());
    };

    let Err(pull_error) = process::run_checked(&["pull".to_owned(), reference.to_owned()]) else {
        return Ok(reference.to_owned());
    };

    if std::env::var(gate::REQUIRE_ENV).as_deref() == Ok("1") {
        return Err(DockerError::required_digest_missing(
            reference,
            digest,
            &pull_error,
        ));
    }

    if image_present(tag)? {
        eprintln!(
            "fabric-ndc-acceptance: {reference} could not be pulled ({pull_error}); falling back \
             to the bare tag {tag} (see images.rs for why)"
        );
        return Ok(tag.to_owned());
    }

    Ok(reference.to_owned())
}

impl DockerError {
    /// Built by [`resolve_runnable_reference`] when the required mode
    /// refuses the bare-tag fallback after the pull itself failed. Carries
    /// the pull's own failure detail (command line and `stderr`) alongside
    /// the digest, so the caller sees both what was missing and why fetching
    /// it did not fix that.
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

#[cfg(test)]
mod tests {
    use super::repository_digest_form;

    #[test]
    fn a_digest_qualified_reference_drops_its_tag() {
        assert_eq!(
            repository_digest_form("ghcr.io/hasura/ndc-postgres:v3.1.0@sha256:abc"),
            Some("ghcr.io/hasura/ndc-postgres@sha256:abc".to_owned())
        );
    }

    #[test]
    fn a_bare_tag_has_no_digest_form() {
        assert_eq!(repository_digest_form("postgres:16-alpine"), None);
    }
}
