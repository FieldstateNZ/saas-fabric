//! Finding the header a manifest was written under.

/// The leading comment block, including any document separator.
///
/// Stops at the first line that is content. A blank line inside the comment
/// block is kept, because a header written in paragraphs should stay in
/// paragraphs; a blank line *after* it is not, because the renderer supplies
/// its own layout below.
pub(super) fn header_of(text: &str) -> String {
    let mut header = String::new();
    let mut pending_blanks = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            pending_blanks.push_str(line);
            pending_blanks.push('\n');
            continue;
        }

        if trimmed.starts_with('#') || trimmed == "---" {
            header.push_str(&pending_blanks);
            pending_blanks.clear();
            header.push_str(line);
            header.push('\n');
            continue;
        }

        break;
    }

    header
}
