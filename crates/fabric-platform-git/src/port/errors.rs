//! Mapping this adapter's failures into the port's vocabulary.

use fabric_platform_management::DesiredStateError;

use crate::PlatformGitError;

/// Maps this adapter's failures into the port's vocabulary.
///
/// A free function's worth of translation, and the distinctions that matter
/// survive it. `Conflict` in particular has to: it is not a failure of the
/// component but an instruction to decide again, and a caller that could not
/// tell it from an outage would either retry forever or give up on a race it
/// was always going to lose once.
impl From<PlatformGitError> for DesiredStateError {
    fn from(error: PlatformGitError) -> Self {
        match error {
            PlatformGitError::Conflict { .. } => Self::Conflict,
            PlatformGitError::Contended => Self::Unavailable {
                detail: "the platform repository is busy".to_owned(),
            },
            PlatformGitError::NotFound { what } => Self::NotFound { what },
            PlatformGitError::NotPermitted => Self::Refused {
                detail: "the platform repository refused the platform's credential".to_owned(),
            },
            PlatformGitError::Unavailable { detail } => Self::Unavailable { detail },
            PlatformGitError::Rejected { detail } => Self::Refused { detail },
        }
    }
}
