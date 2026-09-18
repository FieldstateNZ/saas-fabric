//! Reading the revision a write claims to be editing.

use fabric_client_model::ClientRevision;
use fabric_platform_management::DesiredRevision;
use http::header::{IF_MATCH, IF_NONE_MATCH};
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
    let unquoted = required_tag(headers)?;

    ClientRevision::try_new(unquoted).map_err(|_| ControlPlaneError::RevisionRequired)
}

/// The single strong entity tag `If-Match` carries, syntactically checked
/// but not yet turned into any particular revision type.
///
/// Every revision this API reads back from `If-Match` shares the same
/// header-level rules — see this module's doc comment for what each refusal
/// above means — so this is the one place that parses the header, and
/// [`required_revision`] and [`required_platform_revision`] each turn the
/// result into their own opaque type.
fn required_tag(headers: &HeaderMap) -> Result<&str, ControlPlaneError> {
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

    Ok(value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value))
}

/// [`required_revision`]'s sibling for Platform Management's own opaque
/// revision, carried on `environments/<environment>/data-sources.yaml`
/// (ADR 0023 part 1) rather than on a client document.
///
/// `DesiredRevision::new` is infallible — it is an opaque token an adapter
/// compares by equality and never parses — so, unlike [`ClientRevision`],
/// there is no shape to reject here beyond the header-level rules
/// [`required_tag`] already enforces.
pub(crate) fn required_platform_revision(headers: &HeaderMap) -> Result<DesiredRevision, ControlPlaneError> {
    required_tag(headers).map(DesiredRevision::new)
}

/// Extracts the revision a conditional create-or-replace expects, or `None`
/// when the caller declared "only if it does not yet exist".
///
/// # Why this differs from [`required_revision`]
///
/// Creating the catalogue for the first time has no prior revision to name —
/// `If-Match` cannot express "nothing is here yet". `If-None-Match: *` is the
/// header this situation exists for, so it is accepted here, and nowhere else
/// in this crate: every other write in the API is a replace, which always has
/// a revision to name.
///
/// `If-Match` wins whenever both are present. A caller sending both is asking
/// for "replace this specific revision" and "create if absent" at once, and
/// a document that exists at the named revision satisfies both readings — so
/// there is no case where honouring `If-Match` disagrees with what
/// `If-None-Match: *` alone would have meant.
///
/// # Errors
///
/// Returns [`ControlPlaneError::RevisionRequired`] under the same rules as
/// [`required_revision`] whenever `If-Match` is the header that decides this
/// request — including when neither header is present at all.
pub(crate) fn optional_revision(headers: &HeaderMap) -> Result<Option<ClientRevision>, ControlPlaneError> {
    if wants_create_only(headers) {
        return Ok(None);
    }

    required_revision(headers).map(Some)
}

/// Whether the caller declared "only if it does not yet exist" rather than
/// naming a revision to replace — the one check [`optional_revision`] uses.
/// Platform Management's own writes have no sibling that accepts this: every
/// environment always has a revision to name, even one with nothing declared
/// yet, so [`required_platform_revision`] is the only way in for those.
fn wants_create_only(headers: &HeaderMap) -> bool {
    headers.get(IF_NONE_MATCH).is_some_and(|value| value == "*") && !headers.contains_key(IF_MATCH)
}

/// Renders a revision as a strong entity tag.
///
/// Strong, because two documents with the same revision are byte-identical by
/// construction — the revision *is* a function of the content.
pub(crate) fn entity_tag(revision: &ClientRevision) -> String {
    format!("\"{revision}\"")
}

/// [`entity_tag`]'s sibling for Platform Management's own revision.
pub(crate) fn platform_entity_tag(revision: &DesiredRevision) -> String {
    format!("\"{}\"", revision.as_str())
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

    fn if_none_match_star() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, http::HeaderValue::from_static("*"));
        headers
    }

    #[test]
    fn if_match_only_names_the_revision_to_replace() {
        let revision = optional_revision(&if_match("\"abc123\"")).unwrap();

        assert_eq!(revision.unwrap().as_str(), "abc123");
    }

    #[test]
    fn if_none_match_star_only_means_create_if_absent() {
        assert_eq!(optional_revision(&if_none_match_star()).unwrap(), None);
    }

    #[test]
    fn both_present_lets_if_match_win() {
        let mut headers = if_match("\"abc123\"");
        headers.insert(IF_NONE_MATCH, http::HeaderValue::from_static("*"));

        let revision = optional_revision(&headers).unwrap();

        assert_eq!(revision.unwrap().as_str(), "abc123");
    }

    #[test]
    fn neither_header_is_refused() {
        assert!(matches!(
            optional_revision(&HeaderMap::new()),
            Err(ControlPlaneError::RevisionRequired)
        ));
    }

    #[test]
    fn a_platform_revision_accepts_any_syntactically_unambiguous_token() {
        // Unlike `ClientRevision`, `DesiredRevision` does not validate a
        // character set -- it is opaque all the way down, so the only
        // refusals left are the header-level ones every revision shares.
        let revision = required_platform_revision(&if_match("\"rev-1\"")).unwrap();

        assert_eq!(revision.as_str(), "rev-1");
    }

    #[test]
    fn a_platform_wildcard_is_refused_rather_than_treated_as_any_revision() {
        assert!(required_platform_revision(&if_match("*")).is_err());
    }

    #[test]
    fn a_platform_entity_tag_round_trips() {
        let revision = DesiredRevision::new("rev-1");
        let tag = platform_entity_tag(&revision);

        assert_eq!(required_platform_revision(&if_match(&tag)).unwrap(), revision);
    }
}
