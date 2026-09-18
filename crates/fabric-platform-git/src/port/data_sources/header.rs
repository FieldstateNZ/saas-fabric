//! The leading comment block a data-sources document was found under.

/// The leading comment block, including any document separator. Copied
/// from `components::document::header_of` rather than shared: this crate's
/// rule is that components.rs must not know about data sources, and this
/// is the smaller cost of keeping that true.
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
