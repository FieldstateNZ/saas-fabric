//! GitHub's contents API per-file size ceiling, enforced before any write
//! reaches a repository.
//!
//! GitHub's contents API serves a file's content only up to 1 MB; a bigger
//! file's `content` field comes back empty regardless of what is actually
//! stored — "Between 1-100 MB: … the `content` field will be an empty
//! string and the `encoding` field will be `none`" (GitHub's REST API
//! reference for repository contents). A document that grows past that is
//! not a document this platform could read back, so it is refused before it
//! is ever written rather than discovered the next time someone tries to
//! read it.
//!
//! # Why the limit is short of GitHub's own
//!
//! Every publish adds another full definition snapshot to the catalogue, and
//! every operator action appends to an activity feed neither bounds — so a
//! document that fits today does not necessarily fit next month. Checked at
//! 900 KiB rather than at the full 1 MiB, so there is headroom between what
//! this estimate measures and the bytes GitHub actually receives (UTF-8
//! multi-byte characters, an adapter's own framing) for that gap to never be
//! the reason a write this check accepted is the one GitHub truncates.

use crate::ControlPlaneError;

/// The largest rendered document this platform will write.
const MAX_DOCUMENT_BYTES: usize = 900 * 1024;

/// Refuses a document whose rendered text would not round-trip through
/// GitHub's contents API.
///
/// Called with the text a document or catalogue has just rendered to,
/// *before* that same object is handed to a repository for the actual write
/// — so a caller pays one extra render to measure, on a path already making
/// a network call.
///
/// # Errors
///
/// Returns [`ControlPlaneError::DocumentTooLarge`] if `text` is longer than
/// [`MAX_DOCUMENT_BYTES`].
pub(crate) fn check(text: &str) -> Result<(), ControlPlaneError> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err(ControlPlaneError::DocumentTooLarge {
            limit: MAX_DOCUMENT_BYTES,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_at_the_limit_is_accepted() {
        check(&"a".repeat(MAX_DOCUMENT_BYTES)).unwrap();
    }

    #[test]
    fn a_document_one_byte_over_the_limit_is_refused() {
        let error = check(&"a".repeat(MAX_DOCUMENT_BYTES + 1)).unwrap_err();

        assert!(matches!(
            error,
            ControlPlaneError::DocumentTooLarge { limit } if limit == MAX_DOCUMENT_BYTES
        ));
    }
}
