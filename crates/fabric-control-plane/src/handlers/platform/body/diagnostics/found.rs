//! The one detail of an invalid reason the console is sent.

use fabric_platform_management::InvalidReason;

/// The longest format version sent: `v` and up to fifteen digits.
const MAX_FOUND: usize = 16;

/// The format version an `unsupportedVersion` found, when it is `v` and a
/// bounded run of digits; nothing for any other reason.
pub(super) fn found(reason: &InvalidReason) -> Option<String> {
    match reason {
        InvalidReason::UnsupportedVersion { found } => {
            let digits = found.strip_prefix('v')?;
            let plain = !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit());
            (plain && found.len() <= MAX_FOUND).then(|| found.clone())
        }
        InvalidReason::Unreadable
        | InvalidReason::WrongVersion
        | InvalidReason::Several
        | InvalidReason::PrimaryNotNamed
        | InvalidReason::OtherRegistry
        | InvalidReason::MissingImage { .. }
        | InvalidReason::NoSingleRevision { .. }
        | InvalidReason::NotRegistered { .. }
        | InvalidReason::NotPinned => None,
    }
}
