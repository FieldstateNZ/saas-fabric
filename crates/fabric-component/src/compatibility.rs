//! SaaS Fabric's own rollback floor: the oldest release that may be rolled
//! back to (D01-21, #124).
//!
//! `compatibility.yaml` at the repository root is the authored record, and
//! `tests/this_repositorys_compatibility.rs` holds these constants to it on
//! every pull request. They are constants rather than a parse of that file at
//! start-up because a binary must carry the floor it was built with. A file
//! read at run time could be edited to admit a rollback the binary cannot
//! survive.

/// The repository of the image that carries SaaS Fabric's component
/// descriptor, as `component.yaml` names its `runtime` role.
///
/// Platform Management applies [`MINIMUM_ROLLABLE_VERSION`] to whichever
/// component of an environment pulls from this repository, because that
/// component is this platform, whatever its desired state calls it.
pub const RUNTIME_REPOSITORY: &str = "ghcr.io/fieldstatenz/saas-fabric";

/// The oldest release of SaaS Fabric that reads everything this release
/// writes, and so the oldest an in-product rollback may restore.
///
/// # Why this version
///
/// Releases up to `0.3.0-preview.15` were cut before three persisted
/// formats existed, and each reads the newer state as something else:
///
/// - **The publication record (#136).** An older publisher ignores it, so
///   readers hold a stale generation until an operator removes the record.
/// - **Resource history in the record (#138).** A publisher from #136
///   refuses it only as unreadable, and one from before #136 drops the
///   history, so a retired resource can return at a lower revision.
/// - **`spec.lifecycle` in client documents (#137).** An older control plane
///   ignores the section, so it treats a retired client as usable again.
///
/// The first release that can include all three is the next one cut from
/// `main`, so the floor is that release. It is expected to carry this floor
/// as well. A release cut from `main` before this floor merged would still
/// read all three formats, so it would still be safe to restore. Nothing can
/// be enforced against a binary older than the floor.
/// That is why the floor is enforced where the newer binary acts: Platform
/// Management will not roll SaaS Fabric back below this version. A publisher
/// from this release also writes
/// [`CONTRACT_VERSION`](fabric_runtime_publication::CONTRACT_VERSION) as the
/// record's publisher floor.
pub const MINIMUM_ROLLABLE_VERSION: &str = "0.3.0-preview.16";
