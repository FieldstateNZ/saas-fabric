//! The rendered layout is a cross-repository contract, so it is pinned.

use super::Document;

/// Exactly what `render` must produce, header and all.
///
/// It lives beside the tests rather than in the platform repository because
/// this crate is what produces it; that repository's manifest is generated to
/// match, and `scripts/check.py` there holds the rest of its content together.
const CANONICAL: &str = include_str!("../../tests/fixtures/components.yaml");

#[test]
fn the_canonical_manifest_round_trips_byte_for_byte() {
    // Without this, reordering a field in `Manifest` would change nothing that
    // fails -- until a routine version bump produced a diff that reformatted
    // the whole file, in a commit whose message said it changed a digest.
    let rendered = Document::parse(CANONICAL)
        .expect("the canonical fixture must parse")
        .render()
        .expect("the canonical fixture must render");

    if rendered != CANONICAL {
        for (number, (expected, actual)) in CANONICAL.lines().zip(rendered.lines()).enumerate() {
            assert_eq!(expected, actual, "line {}", number + 1);
        }
        panic!(
            "the rendering differs in length: fixture {} bytes, rendered {} bytes",
            CANONICAL.len(),
            rendered.len()
        );
    }
}

#[test]
fn a_hold_survives_the_round_trip_with_its_note() {
    let manifest = Document::parse(CANONICAL).expect("parses").manifest;
    let hold = manifest.components["keycloak"]
        .hold
        .as_ref()
        .expect("the fixture holds keycloak");

    assert_eq!(hold.reason, "rollback");
    assert_eq!(hold.note.as_deref(), Some("26.8.0 broke the operator console"));
    assert!(manifest.components["saas-fabric"].hold.is_none());
}
