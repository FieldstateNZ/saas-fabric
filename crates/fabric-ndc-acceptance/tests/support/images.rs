//! Every image this harness runs, pinned by digest in one place.
//!
//! Pin by digest, not by tag alone: a tag can move under a test without
//! anyone noticing, and this harness exists specifically to say what a real
//! connector process does. See `m2-ndc/plan.md` §3.3 and lead decision 5.

/// `ghcr.io/hasura/ndc-postgres`, pinned to the multi-arch manifest-list
/// digest for `v3.1.0` -- the value a normal `docker pull` resolves the tag
/// to (recorded in `m2-ndc/plan.md` §0 and lead decision 5), and the only
/// form that is safe to check in: it lets Docker pick the right
/// platform-specific manifest on any host, `linux/amd64` or `linux/arm64`
/// alike.
///
/// **This machine's daemon cannot pull it -- and does not fail fast when
/// asked to.** Per `m2-ndc/plan.md` §0, the image present here was fetched
/// by hand over the registry's HTTP API and `docker load`ed as a
/// single-platform archive, which records the platform-specific manifest
/// digest
/// (`sha256:d1420789377464908e17c23568a5a0664b61d95afea75d6c5623c7e2cbbe4d8e`
/// for this machine's architecture) rather than the index digest below. A
/// `docker pull` of the index digest does not error on this machine; it
/// hangs, because Docker Desktop's registry route here goes through a proxy
/// that never answers. `docker::run` (via
/// `docker::image_reference::resolve_runnable_reference`) tries a real
/// `docker pull` of this exact reference first, bounded by
/// `image_reference::PULL_DEADLINE` (120 seconds) precisely because of that
/// hang, and falls back to the bare tag only once that pull has failed or
/// timed out -- which is what lets this constant stay the *correct* pin --
/// the one a networked machine, or CI, actually resolves to -- without
/// breaking a sandboxed developer machine that already has the tag loaded
/// under a different digest and cannot pull to fix that. That fallback is
/// itself disabled under `gate::REQUIRE_ENV=1` (see that constant's doc
/// comment): running this crate's tests in required mode on *this* machine,
/// with only the bare tag loaded and no pull possible, spends the full
/// 120-second deadline once per process -- `image_reference.rs`'s
/// per-reference resolution cache means every test after the first reuses
/// that one outcome rather than paying the deadline again -- and then fails,
/// naming the missing digest and the pull's own error, rather than quietly
/// substituting the tag or hanging forever. That is the required mode doing
/// exactly what it is for, not a defect in the harness.
///
/// Bump by pulling normally and re-reading:
/// `docker image inspect --format '{{index .RepoDigests 0}}' ghcr.io/hasura/ndc-postgres:v3.1.0`.
pub const NDC_POSTGRES: &str =
    "ghcr.io/hasura/ndc-postgres:v3.1.0@sha256:f91910ef5107aa80d31d82639e149b7f41f4a5bb3af9a369397d7d5965d79a57";

/// `postgres:16-alpine`, pinned to the digest this machine's
/// `docker image inspect --format '{{index .RepoDigests 0}}' postgres:16-alpine`
/// reported at implementation time (a normal pull, so the fallback
/// [`NDC_POSTGRES`] documents does not apply here). Bump the same way, with
/// the `postgres:16-alpine` tag.
pub const POSTGRES: &str =
    "postgres:16-alpine@sha256:e013e867e712fec275706a6c51c966f0bb0c93cfa8f51000f85a15f9865a28cb";

/// `nginx:1.27-alpine`, standing in for "a real HTTP process that answers
/// `200` but is not an NDC connector" (see `impostor.rs`). Pinned the same
/// way as [`POSTGRES`]; bump with the `nginx:1.27-alpine` tag.
pub const NGINX: &str =
    "nginx:1.27-alpine@sha256:65645c7bb6a0661892a8b03b89d0743208a18dd2f3f17a54ef4b76fb8e2f2a10";
