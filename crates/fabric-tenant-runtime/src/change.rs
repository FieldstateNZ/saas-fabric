//! Notifications that a tenant's binding has moved.

use fabric_core::{BindingRevision, TenantId};

/// What happened to a tenant's binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingChangeKind {
    /// A tenant the registry had never seen.
    Added,
    /// An existing tenant's binding advanced to a new revision.
    Updated,
    /// A tenant disappeared from the source, or was invalidated.
    Removed,
}

impl BindingChangeKind {
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

/// A single binding transition, broadcast to interested subscribers.
///
/// # What this is for
///
/// The layer below the registry attaches resources to a binding — a resolved
/// credential, a cached execution target, eventually a connector-side pool.
/// When the binding moves, those resources belong to a version of reality that
/// no longer exists.
///
/// This is the signal to let them go. It is what makes §19's live migration
/// work: provision the new database, migrate the data, publish revision N+1,
/// and everything attached to revision N gets retired without an application
/// deployment or a restart.
///
/// Delivered over a broadcast channel, so a slow subscriber lags rather than
/// blocking the registry. A lagging subscriber has missed transitions and
/// should re-read the binding it cares about rather than assume it is current —
/// which is safe, because the registry always holds the latest state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingChange {
    /// The tenant whose binding moved.
    pub tenant: TenantId,

    /// What kind of transition this was.
    pub kind: BindingChangeKind,

    /// The revision before the change, if there was one.
    pub previous_revision: Option<BindingRevision>,

    /// The revision after the change. `None` for a removal.
    pub current_revision: Option<BindingRevision>,
}

impl BindingChange {
    /// A tenant appearing for the first time.
    #[must_use]
    pub const fn added(tenant: TenantId, revision: BindingRevision) -> Self {
        Self {
            tenant,
            kind: BindingChangeKind::Added,
            previous_revision: None,
            current_revision: Some(revision),
        }
    }

    /// A tenant's binding advancing.
    #[must_use]
    pub const fn updated(tenant: TenantId, previous: BindingRevision, current: BindingRevision) -> Self {
        Self {
            tenant,
            kind: BindingChangeKind::Updated,
            previous_revision: Some(previous),
            current_revision: Some(current),
        }
    }

    /// A tenant going away.
    #[must_use]
    pub const fn removed(tenant: TenantId, previous: BindingRevision) -> Self {
        Self {
            tenant,
            kind: BindingChangeKind::Removed,
            previous_revision: Some(previous),
            current_revision: None,
        }
    }
}
