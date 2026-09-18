//! The leading comment block a placements document was found under.

/// The leading comment block, including any document separator. Copied
/// from `port::data_sources::header::header_of` rather than shared: that
/// copy was itself copied from `components::document::header_of` for the
/// same reason -- `components.rs` must not know about data sources, and
/// neither of those may know about placements. Duplicating eight lines is
/// the smaller cost.
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
