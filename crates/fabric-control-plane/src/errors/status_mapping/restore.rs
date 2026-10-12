//! Which status, and which stable code, a refused restore or a continuity
//! hold carries.
//!
//! | Code | Status | When |
//! |---|---|---|
//! | `placements_unavailable` | 503 | the placements, or what was published from them, could not be read |
//! | `restore_placements_not_retained` | 409 | a placement was removed, replaced, lowered or orphaned while the client was retired |
//! | `restore_placements_unverified` | 409 | no platform or publication target is configured, or the target holds no accepted publication |
//! | `lifecycle_continuity_unavailable` | 503 | no protected continuity store exists (held; no `Retry-After`), or it could not be reached |
//! | `lifecycle_continuity_unenrolled` | 409 | the store holds no accepted lifecycle for the client |
//! | `lifecycle_continuity_mismatch` | 409 | the document's lifecycle is not the one the store accepted |
//! | `lifecycle_continuity_pending` | 409 | another lifecycle transition is reserved and not confirmed |
//! | `lifecycle_continuity_resurrection` | 409 | the store already knows the client id |

use http::StatusCode;

use crate::{ContinuityHold, RestoreRefusal};

impl RestoreRefusal {
    /// The status an operator's browser sees.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::PlacementsUnreadable => StatusCode::SERVICE_UNAVAILABLE,
            Self::PlacementsNotRetained { .. } | Self::PlacementsUnverified => StatusCode::CONFLICT,
        }
    }

    /// A stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PlacementsUnreadable => "placements_unavailable",
            Self::PlacementsNotRetained { .. } => "restore_placements_not_retained",
            Self::PlacementsUnverified => "restore_placements_unverified",
        }
    }
}

impl ContinuityHold {
    /// The status an operator's browser sees.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::NoStore | Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Unenrolled | Self::Mismatch | Self::Pending | Self::Resurrection => StatusCode::CONFLICT,
        }
    }

    /// A stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NoStore | Self::Unavailable => "lifecycle_continuity_unavailable",
            Self::Unenrolled => "lifecycle_continuity_unenrolled",
            Self::Mismatch => "lifecycle_continuity_mismatch",
            Self::Pending => "lifecycle_continuity_pending",
            Self::Resurrection => "lifecycle_continuity_resurrection",
        }
    }

    /// Whether asking again soon could change the answer. A deployment with
    /// no continuity store will not grow one by waiting.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }
}
