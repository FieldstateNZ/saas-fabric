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
