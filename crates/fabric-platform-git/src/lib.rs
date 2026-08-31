//! Atomic desired-state mutation in the platform repository.
//!
//! ```text
//! update_files_atomically(base, changes)      one desired-state change
//!       ↓
//! blobs → tree → commit → ref                 ← the translation is here, and only here
//!       ↓
//! one commit on the integration branch
//! ```
//!
//! # Why atomicity is the whole point
//!
//! Moving a component's version touches more than one file — the version
//! record and every overlay that pins one of its images. Written as separate
//! commits, the branch passes through states nobody chose: a record naming a
//! version the overlays do not deploy, or two of three images moved. Argo CD
//! is entitled to reconcile any of them, because a commit on the branch an
//! environment follows *is* desired state.
//!
//! So every change this crate makes is one tree, one commit, one ref update.
//!
//! # The concurrency primitive is the parent commit
//!
//! GitHub's update-a-reference endpoint takes no expected-old-SHA. It takes the
//! new SHA and `force`, and with `force=false` it requires a fast-forward — so
//! a commit whose parent is the head that was read cannot fast-forward a head
//! that has since moved, and the host answers `409`.
//!
//! That is the only concurrency signal, and it is deliberately not the end of
//! the story:
//!
//! ```text
//! 409
//!  ↓
//! re-read the head, and the revisions of the paths being written
//!  ├─ unchanged → rebuild on the new head and retry
//!  └─ changed   → Conflict, and nothing is written
//! ```
//!
//! An unrelated commit to the platform repository therefore costs a retry, and
//! never becomes an operator's problem. A change to a file this write is
//! editing is refused rather than overwritten. That is the same distinction the
//! clients adapter makes with per-file blob revisions, reached differently
//! because the unit of change here is a *set* of files.
//!
//! # There is no `force`
//!
//! No path in this crate sends `force: true`, and no caller can ask for one.
//! Forcing is how a platform repository loses a commit nobody knew about.
//!
//! # Unreachable objects are left alone
//!
//! A losing attempt has usually already created blobs, a tree and a commit
//! before the ref update is refused. Those objects are unreachable and
//! harmless; Git hosts collect them on their own schedule. Deleting them would
//! add failure modes to the recovery path of a failure, to tidy something
//! nobody can see.

mod atomic;
mod components;
mod config;
mod desired;
mod errors;
mod host;
mod model;

pub use components::{Component, Desired, Hold, ImagePin, Manifest, UpdatePolicy, SCHEMA_VERSION};
pub use config::PlatformRepositoryConfig;
pub use desired::{ComponentVersion, ImageDigest};
pub use errors::PlatformGitError;
pub use host::PlatformGitRepository;
pub use model::{CommitRevision, FileChange, FileRevision, StoredFile};
