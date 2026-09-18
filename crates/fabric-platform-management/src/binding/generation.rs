//! Marking a read with the binding it was read through.

use crate::{DesiredRevision, DesiredStateError};

/// Separates the binding generation from the adapter's own revision.
///
/// The adapter's half may contain this character too — a commit revision is
/// whatever the adapter says it is — so the tag is read from the *first* one
/// and everything after it is handed back untouched.
const SEPARATOR: char = ':';

/// Marks a revision with the generation of the binding that produced it.
///
/// # Why the revision carries this at all
///
/// A revision already answers "did the manifest move between the read and the
/// write". It cannot answer "is this still the same repository", because an
/// adapter only ever sees its own repository and every revision it hands out
/// looks current to it. So a decision read through repository A, applied after
/// an operator rebound the platform to repository B, would present a revision B
/// has never heard of — and B would be free to interpret it however it liked.
///
/// Tagging closes that. The binding stamps the read with which binding it was,
/// demands the same stamp back on the write, and strips it before the adapter
/// ever sees it. [`DesiredRevision`] is opaque by contract — constructed and
/// compared, never parsed — so the tag is invisible to the service, the console
/// and the adapter alike.
pub(super) fn tag(generation: u64, revision: &DesiredRevision) -> DesiredRevision {
    DesiredRevision::new(format!("{generation}{SEPARATOR}{}", revision.as_str()))
}

/// Recovers the adapter's revision, refusing one from any other generation.
///
/// # Why a mismatch is a conflict and not a refusal
///
/// [`Conflict`](DesiredStateError::Conflict) means "the state you decided
/// against has moved, decide again". That is exactly what has happened: which
/// repository the platform targets is part of the state a decision is taken
/// against, so a disconnect or a rebind moves it just as surely as somebody
/// adding a hold does. The caller's next step is identical — read again, decide
/// again — and a sweep already knows how to do that.
///
/// A refusal would say something different and worse: that the request was
/// wrong. Nobody did anything wrong. The operator rebound the platform, which
/// they are entitled to do, and the decision in flight simply no longer applies.
pub(super) fn untag(generation: u64, at: &DesiredRevision) -> Result<DesiredRevision, DesiredStateError> {
    let (tagged, revision) = at
        .as_str()
        .split_once(SEPARATOR)
        .ok_or(DesiredStateError::Conflict)?;

    if tagged.parse::<u64>() != Ok(generation) {
        return Err(DesiredStateError::Conflict);
    }

    Ok(DesiredRevision::new(revision))
}

/// The payload `tag_presence` writes for a read that found no file.
const ABSENT: &str = "absent";

/// The payload prefix `tag_presence` writes for a read that found one,
/// right before the adapter's own revision text.
const PRESENT: &str = "present:";

/// [`tag`]'s sibling for a read that may or may not have found a file.
///
/// # Why an absent file still needs a tag
///
/// [`tag`] only has something to embed a generation *in* when the adapter
/// handed back a revision. A read that found no file has nothing -- and a
/// create decided from that bare `None` would reach `write_data_sources`
/// carrying no generation at all, so a rebind between the read and the
/// write could let it land in a repository the decision was never taken
/// about. That is the same failure [`untag`] already refuses for a
/// replace; tagging the absence too closes it for a create, by never
/// handing back a bare `None` in the first place.
pub(super) fn tag_presence(generation: u64, revision: Option<&DesiredRevision>) -> DesiredRevision {
    let payload = revision.map_or_else(
        || ABSENT.to_owned(),
        |revision| format!("{PRESENT}{}", revision.as_str()),
    );

    DesiredRevision::new(format!("{generation}{SEPARATOR}{payload}"))
}

/// [`untag`]'s sibling: recovers what the adapter should see -- `None` to
/// create, `Some` to replace -- refusing a token from any other
/// generation exactly as `untag` does.
pub(super) fn untag_presence(
    generation: u64,
    at: &DesiredRevision,
) -> Result<Option<DesiredRevision>, DesiredStateError> {
    let (tagged, payload) = at
        .as_str()
        .split_once(SEPARATOR)
        .ok_or(DesiredStateError::Conflict)?;

    if tagged.parse::<u64>() != Ok(generation) {
        return Err(DesiredStateError::Conflict);
    }

    if payload == ABSENT {
        return Ok(None);
    }

    payload
        .strip_prefix(PRESENT)
        .map(|revision| Some(DesiredRevision::new(revision)))
        .ok_or(DesiredStateError::Conflict)
}
