//! The value token on a `key: value` line: where it ends, and what may replace it.

/// Splits a value from what follows it, leaving both exactly as written.
///
/// `None` means the line is not one this can read as a key at all — a quote
/// that never closes, an escape, a second quoted run behind the first. Reading
/// an escape would mean implementing YAML's, and writing the value back would
/// mean implementing it in reverse; neither belongs in a version bump. There is
/// deliberately no lossy answer: a shape this cannot read must not come back as
/// a shorter line.
pub(super) fn split(tail: &str) -> Option<(&str, &str)> {
    if tail.is_empty() || tail.starts_with('#') {
        return Some(("", tail));
    }

    for quote in ['"', '\''] {
        let Some(body) = tail.strip_prefix(quote) else {
            continue;
        };
        let end = body.find(quote)?;

        // A backslash inside double quotes starts an escape. `''` inside single
        // quotes is one too, and shows up as a closing quote with more quoted
        // text behind it — which the separator test below sees.
        if quote == '"' && body.get(..end)?.contains('\\') {
            return None;
        }

        let (value, suffix) = tail.split_at_checked(end + 2)?;
        return separated(suffix).then_some((value, suffix));
    }

    // A plain scalar runs to a comment or the end of the line, and every byte
    // between the two belongs to the suffix so that it comes back.
    let head = match comment_at(tail) {
        Some(at) => tail.split_at_checked(at)?.0,
        None => tail,
    };
    tail.split_at_checked(head.trim_end().len())
}

/// Whether `version` can be written into YAML and read back as the same word.
///
/// A whitelist rather than a blacklist, because the only versions worth writing
/// are the ones a chart repository publishes: the identifier alphabet semantic
/// versioning allows, which `Version::parse_chart` already enforces on the way
/// in. A blacklist has to imagine every
/// character that could hurt, and `7.3.1:` or `]7.3.1` are enough to produce a
/// file that is not YAML — written into a repository that deploys itself, by a
/// caller whose `Chart { version }` is an unvalidated `String`.
pub(super) fn is_writable(version: &str) -> bool {
    !version.is_empty()
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
}

/// Where a comment starts: the first `#` with separation in front of it.
///
/// YAML's separation is a run of spaces **or tabs**, not the single space it is
/// tempting to look for. A `#` with no separation before it is part of the
/// value — `7.3.0#1` is one plain scalar — and one at the start of the value
/// was handled before this is reached.
fn comment_at(tail: &str) -> Option<usize> {
    tail.char_indices().find_map(|(at, ch)| {
        let separated = tail.get(..at).is_some_and(|before| before.ends_with([' ', '\t']));
        (ch == '#' && separated).then_some(at)
    })
}

/// Whether what follows a quoted value is separation rather than more content.
fn separated(suffix: &str) -> bool {
    suffix.is_empty() || suffix.starts_with([' ', '\t'])
}
