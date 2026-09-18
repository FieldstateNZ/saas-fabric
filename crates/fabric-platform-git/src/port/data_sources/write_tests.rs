//! `CREATE_HEADER` is the cross-repository contract for a brand-new file,
//! pinned the same way the canonical fixture pins render's ongoing output.

use super::CREATE_HEADER;
use crate::port::data_sources::document::Document;

/// The same fixture document.rs's own tests pin `render` against.
const CANONICAL: &str = include_str!("../../../tests/fixtures/data-sources.yaml");

#[test]
fn a_created_document_carries_the_create_header_verbatim() {
    let document = Document::new(CREATE_HEADER.to_owned(), "lucentroot", &[]);
    let rendered = document.render().expect("an empty declaration list renders");

    assert!(
        rendered.starts_with(CREATE_HEADER),
        "a created document's header must be exactly CREATE_HEADER: {rendered}"
    );
}

#[test]
fn the_create_headers_first_line_matches_the_fixtures() {
    // The fixture is what an existing, hand-or-Fabric-written file looks
    // like; the create header is what Fabric writes when there was none.
    // The two must open with the same sentence, or an operator would see
    // the file's own description change depending on how it came to exist.
    let fixture_first_line = CANONICAL.lines().next().expect("the fixture has a first line");
    let header_first_line = CREATE_HEADER.lines().next().expect("the header has a first line");

    assert_eq!(header_first_line, fixture_first_line);
}
