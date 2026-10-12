//! Which status, and which stable code, a refused restore carries.
//!
//! | Code | Status | When |
//! |---|---|---|
//! | `lifecycle_continuity_unavailable` | 503 | no protected continuity store exists (held; no `Retry-After`), or it could not be reached |
//! | `lifecycle_continuity_unenrolled` | 409 | the store holds no accepted lifecycle for the client |
//! | `lifecycle_continuity_mismatch` | 409 | the store's accepted lifecycle does not end at the document's |
//! | `placements_unavailable` | 503 | the environment's placements could not be read |
//! | `restore_placements_not_retained` | 409 | a placement was removed, lowered or orphaned while the client was retired |
//! | `restore_placements_unverified` | 409 | no publication pass has published since start, so nothing to compare with |

use http::StatusCode;

use crate::RestoreRefusal;

impl RestoreRefusal {
    /// The status an operator's browser sees.
    #[allow(
        clippy::match_same_arms,
        reason = "arms are grouped by cause; the code keeps each apart"
    )]
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        match self {
            Self::ContinuityAbsent | Self::ContinuityUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::PlacementsUnreadable => StatusCode::SERVICE_UNAVAILABLE,
            Self::ContinuityUnenrolled | Self::ContinuityMismatch => StatusCode::CONFLICT,
            Self::PlacementsNotRetained { .. } | Self::PlacementsUnverified => StatusCode::CONFLICT,
        }
    }

    /// A stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ContinuityAbsent | Self::ContinuityUnavailable => "lifecycle_continuity_unavailable",
            Self::ContinuityUnenrolled => "lifecycle_continuity_unenrolled",
            Self::ContinuityMismatch => "lifecycle_continuity_mismatch",
            Self::PlacementsUnreadable => "placements_unavailable",
            Self::PlacementsNotRetained { .. } => "restore_placements_not_retained",
            Self::PlacementsUnverified => "restore_placements_unverified",
        }
    }

    /// Whether asking again soon could change the answer. A deployment with
    /// no continuity store will not grow one by waiting.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        !matches!(self, Self::ContinuityAbsent)
    }
}
