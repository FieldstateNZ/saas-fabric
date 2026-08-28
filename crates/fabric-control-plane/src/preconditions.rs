//! Reading the revision a write claims to be editing.

use fabric_client_model::ClientRevision;
use http::header::IF_MATCH;
use http::HeaderMap;

use crate::ControlPlaneError;

/// Extracts the revision from `If-Match`, refusing anything ambiguous.
///
/// # Why `If-Match` and not a field in the body
///
/// Because this is exactly what the header is for, and because putting it in
/// the body makes it optional in practice: a client that forgot the field
/// would send a syntactically valid request, and the only way to stop it
/// silently overwriting someone else's change is a check that is easy to
/// forget to write. A missing header is impossible to overlook.
///
/// # What is refused, and why each one matters
///
/// - **Absent.** Answered `428 Precondition Required`, not a blind write. A
///   write with no expectation is last-writer-wins, which ADR 0008 forbids.
/// - **`*`.** Means "if the resource exists", which is not a revision. It
///   would let a client opt out of concurrency control by sending one
///   character, so it is refused rather than honoured.
/// - **Weak tags (`W/"…"`).** Weak comparison permits two entities that are
///   *equivalent* to match. Nothing about a desired-state document is
///   equivalent-but-different; a weak match here would be a lost update.
/// - **More than one tag.** `If-Match` permits a list, and a list means "any
///   of these". There is exactly one revision a caller can have read, so a
///   list is a client that does not know which one it edited.
///
/// # Errors
///
/// Returns [`ControlPlaneError::RevisionRequired`] for every case above,
/// including a value that is not a legal revision. They share one error
/// because they share one remedy: read the resource and send its entity tag.
pub(crate) fn required_revision(headers: &HeaderMap) -> Result<ClientRevision, ControlPlaneError> {
    let mut values = headers.get_all(IF_MATCH).iter();

    let value = values.next().ok_or(ControlPlaneError::RevisionRequired)?;
    if values.next().is_some() {
        return Err(ControlPlaneError::RevisionRequired);
    }

    let value = value
        .to_str()
        .map_err(|_| ControlPlaneError::RevisionRequired)?
        .trim();

    if value.contains(',') || value.starts_with("W/") || value == "*" {
        return Err(ControlPlaneError::RevisionRequired);
    }

    let unquoted = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value);

    ClientRevision::try_new(unquoted).map_err(|_| ControlPlaneError::RevisionRequired)
}

/// Renders a revision as a strong entity tag.
///
/// Strong, because two documents with the same revision are byte-identical by
/// construction — the revision *is* a function of the content.
pub(crate) fn entity_tag(revision: &ClientRevision) -> String {
    format!("\"{revision}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn if_match(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = http::HeaderValue::from_str(value) {
            headers.insert(IF_MATCH, value);
        }
        headers
    }

    #[test]
    fn a_quoted_revision_is_accepted() {
        let revision = required_revision(&if_match("\"abc123\"")).unwrap();

        assert_eq!(revision.as_str(), "abc123");
    }

    #[test]
    fn an_unquoted_revision_is_accepted_too() {
        // Not strictly a legal entity tag, but refusing it would only punish a
        // client that is being clear about what it read.
        assert!(required_revision(&if_match("abc123")).is_ok());
    }

    #[test]
    fn a_missing_header_is_refused() {
        assert!(matches!(
            required_revision(&HeaderMap::new()),
            Err(ControlPlaneError::RevisionRequired)
        ));
    }

    #[test]
    fn a_wildcard_is_refused_rather_than_treated_as_any_revision() {
        // The one-character opt-out of concurrency control.
        assert!(required_revision(&if_match("*")).is_err());
    }

    #[test]
    fn a_weak_tag_is_refused() {
        assert!(required_revision(&if_match("W/\"abc123\"")).is_err());
    }

    #[test]
    fn a_list_of_tags_is_refused() {
        assert!(required_revision(&if_match("\"abc\", \"def\"")).is_err());
    }

    #[test]
    fn a_value_that_is_not_a_revision_is_refused() {
        assert!(required_revision(&if_match("\"has a space\"")).is_err());
    }

    #[test]
    fn an_entity_tag_round_trips() {
        let revision = ClientRevision::try_new("abc123").unwrap();
        let tag = entity_tag(&revision);

        assert_eq!(required_revision(&if_match(&tag)).unwrap(), revision);
    }
}
