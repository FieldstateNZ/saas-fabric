//! Reading the whole document before anything is decided.

use super::entry::Entry;
use super::lines::Line;
use super::position::{Position, Role};
use super::Refusal;

/// Collects every direct entry of a top-level `spec.sources`.
///
/// # Why the file is read out before it is edited
///
/// A mapping's keys have no order in YAML, so a source that writes
/// `targetRevision` above `repoURL` is the same source written differently. A
/// walk that edited as it went could not know that: reaching the revision, it
/// has not yet seen the repository that would have told it this was the one to
/// move, and it silently moves nothing. The file then looks like it named no
/// source — a refusal, but for a reason that is not true.
///
/// Reading first and deciding afterwards costs one vector and makes key order
/// stop mattering, which is what a YAML author is entitled to expect.
///
/// Every document in the file is walked, because a `---` starts a new one with
/// a `spec:` of its own and the pin may be in either. "Exactly one match"
/// stays a rule about the file, not about each document in it.
///
/// # Errors
///
/// A clause saying why, when a shape the walk depends on cannot be read.
pub(super) fn collect<'a>(lines: &[Line<'a>]) -> Result<Vec<Entry<'a>>, Refusal> {
    let mut position = Position::Outside;
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut open = false;

    for (index, line) in lines.iter().enumerate() {
        if !line.is_significant() {
            continue;
        }

        match position.observe(line)? {
            Role::Opens => {
                entries.push(Entry::opened(index, line));
                open = true;
            }
            Role::Within => {
                if let Some(entry) = entries.last_mut().filter(|_| open) {
                    entry.observe(index, line);
                }
            }
            // Leaving the list closes whatever source was open, so a key after
            // it is never read as part of one.
            Role::Elsewhere => open = false,
        }
    }

    Ok(entries)
}
