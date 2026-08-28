//! Notifications that a resource moved.

use fabric_core::BindingRevision;

/// What happened to a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// A resource the registry had never seen.
    Added,
    /// An existing resource advanced to a new revision.
    Updated,
    /// A resource disappeared from the source, or was invalidated.
    Removed,
}

impl ChangeKind {
    /// A stable name for telemetry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Updated => "updated",
            Self::Removed => "removed",
        }
    }
}

/// A single resource transition, broadcast to interested subscribers.
///
/// # What this is for
///
/// Layers below the registry attach things to a resource — a resolved
/// credential, a cached execution target, eventually a connector-side pool.
/// When the resource moves, those belong to a version of reality that no longer
/// exists.
///
/// This is the signal to let them go, and it is what makes §19's live migration
/// work: provision the new database, migrate the data, publish revision N+1,
/// and everything attached to revision N is retired without an application
/// deployment or a restart.
///
/// Delivered over a broadcast channel, so a slow subscriber lags rather than
/// blocking the registry. A lagging subscriber has missed transitions and
/// should re-read what it cares about — which is safe, because the registry
/// always holds current state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceChange<K> {
    /// Which resource moved.
    pub key: K,

    /// What kind of transition this was.
    pub kind: ChangeKind,

    /// The revision before the change, if there was one.
    pub previous_revision: Option<BindingRevision>,

    /// The revision after the change. `None` for a removal.
    pub current_revision: Option<BindingRevision>,
}

impl<K> ResourceChange<K> {
    /// A resource appearing for the first time.
    #[must_use]
    pub const fn added(key: K, revision: BindingRevision) -> Self {
        Self {
            key,
            kind: ChangeKind::Added,
            previous_revision: None,
            current_revision: Some(revision),
        }
    }

    /// A resource advancing.
    #[must_use]
    pub const fn updated(key: K, previous: BindingRevision, current: BindingRevision) -> Self {
        Self {
            key,
            kind: ChangeKind::Updated,
            previous_revision: Some(previous),
            current_revision: Some(current),
        }
    }

    /// A resource going away.
    #[must_use]
    pub const fn removed(key: K, previous: BindingRevision) -> Self {
        Self {
            key,
            kind: ChangeKind::Removed,
            previous_revision: Some(previous),
            current_revision: None,
        }
    }
}
