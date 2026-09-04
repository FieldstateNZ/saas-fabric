//! Reading the whole document before anything is decided.

use super::entry::Entry;
use super::lines::Line;
use super::position::{Position, Role};
use super::scalar::Scalar;
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
/// a `spec:` of its own and the pin may be in either. "Exactly one match" stays
/// a rule about the file, not about each document in it.
///
/// # Errors
///
/// A clause saying why, when a shape the walk depends on cannot be read, or
/// when a line it has to measure is indented with a tab.
pub(super) fn collect<'a>(lines: &[Line<'a>]) -> Result<Vec<Entry<'a>>, Refusal> {
    let mut position = Position::Outside;
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut open = false;
    let mut block: Option<usize> = None;

    for (index, line) in lines.iter().enumerate() {
        // A block scalar's body is text, not structure. Its indentation is the
        // author's to choose — a pasted script or JSON blob under
        // `helm.values: |` may be laid out with tabs, which is legal YAML
        // because the tabs are content — and a `targetRevision:` inside it is a
        // line of a values file rather than a key of a source. So the body is
        // neither measured nor observed: only a non-blank line back at or above
        // the header's own indent ends it.
        if let Some(header) = block {
            if line.is_blank() || line.indent() > header {
                continue;
            }
            block = None;
        }

        if !line.is_significant() {
            continue;
        }

        if line.is_tab_indented() {
            return Err(format!(
                "is indented with a tab on line {}, and a tab's width is nobody's to \
                 assume, so how deep that line sits cannot be said",
                index + 1
            ));
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

        // The header's indent is the *key's* column, which on a `- key: |`
        // line is not the line's own. Measuring from the dash would put every
        // one of that entry's other keys inside the block and hide them --
        // and an Argo source may open with `ref: >-`, so the second of two
        // sources naming one chart would vanish, and an ambiguity this exists
        // to refuse would be resolved by editing the first.
        let (column, from) = match line.after_dash() {
            Some((column, first)) => (column, first),
            None => (line.indent(), line.rest),
        };
        if Scalar::read(line, from).is_some_and(|key| key.opens_a_block()) {
            block = Some(column);
        }
    }

    Ok(entries)
}
