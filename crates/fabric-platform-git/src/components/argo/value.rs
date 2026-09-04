//! The value token on a `key: value` line: where it ends, and what may replace it.

/// Splits a value from what follows it, leaving both exactly as written.
///
/// An unreadable shape — a quote that never closes, an escape, a second quoted
/// run behind the first — yields an empty value rather than a guess, and an
/// empty value is refused when it is the one that had to move. Reading an
/// escape would mean implementing YAML's, and writing the value back would
/// mean implementing it in reverse; neither belongs in a version bump.
pub(super) fn split(tail: &str) -> (&str, &str) {
    if tail.is_empty() || tail.starts_with('#') {
        return ("", tail);
    }

    for quote in ['"', '\''] {
        let Some(body) = tail.strip_prefix(quote) else {
            continue;
        };
        let Some(end) = body.find(quote) else {
            return ("", tail);
        };

        // A backslash inside double quotes starts an escape. `''` inside single
        // quotes is one too, and shows up as a closing quote with more quoted
        // text behind it — which the suffix test below sees.
        if quote == '"' && body.get(..end).is_some_and(|inner| inner.contains('\\')) {
            return ("", tail);
        }

        let (value, suffix) = cut(tail, end + 2);
        return if suffix.is_empty() || suffix.starts_with(' ') {
            (value, suffix)
        } else {
            ("", tail)
        };
    }

    // A plain scalar runs to a ` #` comment or the end of the line. The spaces
    // before that comment belong to the suffix, so they come back untouched.
    let head = tail.find(" #").map_or(tail, |at| cut(tail, at).0);
    cut(tail, head.trim_end().len())
}

/// Whether `version` can be written into YAML and read back as the same word.
///
/// Whitespace would split it, `#` would start a comment that swallows the rest
/// of the line, a quote would break the author's quoting, and a leading
/// indicator would make it an anchor, an alias or a collection. A chart
/// registry never offers such a version; refusing one is cheaper than writing
/// a file nobody predicted into a repository that deploys itself.
pub(super) fn is_plain(version: &str) -> bool {
    !version.is_empty()
        && !version.starts_with(['&', '*', '!', '|', '>', '{', '[', '%', '@', '`', ','])
        && !version.contains([' ', '\t', '\r', '\n', '#', '"', '\''])
}

/// Splits `text` at `at`, or leaves it whole if that is not a char boundary.
fn cut(text: &str, at: usize) -> (&str, &str) {
    text.split_at_checked(at).unwrap_or((text, ""))
}
