//! GitHub's contents API per-file size ceiling, enforced before any write
//! reaches a repository.
//!
//! GitHub documents that its contents API's JSON media type — the one this
//! platform's Git adapter requests (`Accept: application/vnd.github+json`)
//! — serves a file's full content only up to 1 MB; above that, this adapter
//! cannot read the document back. A document that grows past that is
//! refused before it is ever written, rather than committed and discovered
//! unreadable the next time somebody tries to read it.
//!
//! # Why there are two limits, not one
//!
//! Every publish adds another full definition snapshot to the catalogue,
//! and every write to a client appends to an activity feed neither bounds —
//! so a document that fits today does not necessarily fit next month, and
//! [`MAX_DOCUMENT_BYTES`] is checked well short of GitHub's own 1 MB to
//! leave headroom for that growth. But a client document large enough to
//! hit that ceiling is exactly the document an operator most needs to be
//! able to keep editing: `PUT /identity` removing a compromised redirect
//! URI, say, is a write that *shrinks* the document, and refusing it with
//! the same ceiling that growth is checked against would block the one
//! edit security remediation depends on, at the moment it matters most.
//! [`MAX_REMEDIATION_DOCUMENT_BYTES`] gives an identity edit more room —
//! still short of the 1 MB GitHub documents, so a document this check
//! accepts is still one this adapter can read back — without sharing
//! [`MAX_DOCUMENT_BYTES`]'s headroom for growth an identity edit does not
//! cause.

use crate::ControlPlaneError;

/// The largest rendered document a write that *grows* a document may
/// produce: a create, a product save, or a catalogue command. See this
/// module's own rustdoc for why it is short of GitHub's own 1 MB, and why
/// an identity edit is checked against [`MAX_REMEDIATION_DOCUMENT_BYTES`]
/// instead.
const MAX_DOCUMENT_BYTES: usize = 900 * 1024;

/// The largest rendered document an identity edit may produce. See this
/// module's own rustdoc for why this is higher than [`MAX_DOCUMENT_BYTES`].
const MAX_REMEDIATION_DOCUMENT_BYTES: usize = 1000 * 1024;

/// Refuses a document whose rendered text would not round-trip through
/// GitHub's contents API, against the ordinary limit — see
/// [`check_remediation`] for the higher one an identity edit is checked
/// against instead.
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
    check_against(text, MAX_DOCUMENT_BYTES)
}

/// [`check`]'s sibling for an identity edit, against
/// [`MAX_REMEDIATION_DOCUMENT_BYTES`] rather than [`MAX_DOCUMENT_BYTES`] —
/// see this module's own rustdoc for why an identity edit needs the extra
/// room.
///
/// # Errors
///
/// Returns [`ControlPlaneError::DocumentTooLarge`] if `text` is longer than
/// [`MAX_REMEDIATION_DOCUMENT_BYTES`].
pub(crate) fn check_remediation(text: &str) -> Result<(), ControlPlaneError> {
    check_against(text, MAX_REMEDIATION_DOCUMENT_BYTES)
}

/// Refuses `text` if it is longer than `limit` bytes.
///
/// # Errors
///
/// Returns [`ControlPlaneError::DocumentTooLarge`] naming `limit` as the
/// limit that was exceeded. Answered as `422`, not `413` — see the error
/// variant's own rustdoc for why.
fn check_against(text: &str, limit: usize) -> Result<(), ControlPlaneError> {
    if text.len() > limit {
        return Err(ControlPlaneError::DocumentTooLarge { limit });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_at_the_ordinary_limit_is_accepted() {
        check(&"a".repeat(MAX_DOCUMENT_BYTES)).unwrap();
    }

    #[test]
    fn a_document_one_byte_over_the_ordinary_limit_is_refused() {
        let error = check(&"a".repeat(MAX_DOCUMENT_BYTES + 1)).unwrap_err();

        assert!(matches!(
            error,
            ControlPlaneError::DocumentTooLarge { limit } if limit == MAX_DOCUMENT_BYTES
        ));
    }

    #[test]
    fn a_document_between_the_two_limits_is_refused_ordinarily_but_accepted_for_remediation() {
        let text = "a".repeat(MAX_DOCUMENT_BYTES + 1);

        assert!(check(&text).is_err());
        check_remediation(&text).unwrap();
    }

    #[test]
    fn a_document_at_the_remediation_limit_is_accepted() {
        check_remediation(&"a".repeat(MAX_REMEDIATION_DOCUMENT_BYTES)).unwrap();
    }

    #[test]
    fn a_document_one_byte_over_the_remediation_limit_is_refused() {
        let error = check_remediation(&"a".repeat(MAX_REMEDIATION_DOCUMENT_BYTES + 1)).unwrap_err();

        assert!(matches!(
            error,
            ControlPlaneError::DocumentTooLarge { limit } if limit == MAX_REMEDIATION_DOCUMENT_BYTES
        ));
    }
}
