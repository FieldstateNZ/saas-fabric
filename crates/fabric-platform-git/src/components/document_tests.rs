//! The rendered layout is a cross-repository contract, so it is pinned.

use super::Document;

/// Exactly what `render` must produce, header and all.
///
/// It lives beside the tests rather than in the platform repository because
/// this crate is what produces it; that repository's manifest is generated to
/// match, and `scripts/check.py` there holds the rest of its content together.
const CANONICAL: &str = include_str!("../../tests/fixtures/components.yaml");

/// The same, at schema 3: one described component beside an OCI one.
const CANONICAL_SCHEMA_3: &str = include_str!("../../tests/fixtures/components-schema3.yaml");

/// Parses and renders `fixture`, and fails on the first line that differs.
fn round_trips(fixture: &str) {
    let rendered = Document::parse(fixture)
        .expect("the canonical fixture must parse")
        .render()
        .expect("the canonical fixture must render");

    if rendered != fixture {
        for (number, (expected, actual)) in fixture.lines().zip(rendered.lines()).enumerate() {
            assert_eq!(expected, actual, "line {}", number + 1);
        }
        panic!(
            "the rendering differs in length: fixture {} bytes, rendered {} bytes",
            fixture.len(),
            rendered.len()
        );
    }
}

#[test]
fn the_canonical_manifest_round_trips_byte_for_byte() {
    // Without this, reordering a field in `Manifest` would change nothing that
    // fails -- until a routine version bump produced a diff that reformatted
    // the whole file, in a commit whose message said it changed a digest.
    round_trips(CANONICAL);
}

#[test]
fn the_canonical_schema_3_manifest_round_trips_byte_for_byte() {
    // And it stays schema 3: a file is written back at the version it was
    // read at, never moved by a Fabric release.
    round_trips(CANONICAL_SCHEMA_3);
}

#[test]
fn a_schema_2_file_is_written_back_as_schema_2() {
    let rendered = Document::parse(CANONICAL)
        .expect("parses")
        .render()
        .expect("renders");

    assert!(rendered.contains("\nschemaVersion: 2\n"), "{rendered}");
}

#[test]
fn a_described_component_is_read_with_its_primary() {
    let manifest = Document::parse(CANONICAL_SCHEMA_3).expect("parses").manifest;

    let crate::Artifact::Described {
        primary,
        source_revision,
        images,
    } = &manifest.components["saas-fabric"].artifact
    else {
        panic!("saas-fabric is described in the schema 3 fixture");
    };
    assert_eq!(primary, "runtime");
    assert_eq!(source_revision, "5707f5e725eb2179a255428bc8c08e22d0845f50");
    assert_eq!(images.len(), 3);
    assert!(matches!(
        manifest.components["keycloak"].artifact,
        crate::Artifact::Oci { .. }
    ));
}

#[test]
fn a_described_component_in_a_schema_2_file_is_refused_by_version() {
    let manifest = CANONICAL_SCHEMA_3.replace("schemaVersion: 3", "schemaVersion: 2");

    let Err(crate::PlatformGitError::Rejected { detail }) = Document::parse(&manifest) else {
        panic!("a described component needs schema 3");
    };
    assert!(
        detail.contains("saas-fabric")
            && detail.contains("schemaVersion 2")
            && detail.contains("needs schemaVersion 3"),
        "{detail}"
    );
}

#[test]
fn a_primary_that_is_not_one_of_the_images_is_refused_by_name() {
    let manifest = CANONICAL_SCHEMA_3.replace("primary: runtime", "primary: sidecar");

    let Err(crate::PlatformGitError::Rejected { detail }) = Document::parse(&manifest) else {
        panic!("a primary must be one of the images");
    };
    assert!(
        detail.contains("sidecar") && detail.contains("saas-fabric"),
        "{detail}"
    );
}

#[test]
fn a_described_component_carries_nothing_it_does_not_declare() {
    // `deny_unknown_fields`: a component descriptor's digest written into the
    // file by hand is refused, not ignored -- it is not part of the contract.
    let manifest = CANONICAL_SCHEMA_3.replace(
        "      primary: runtime\n",
        "      primary: runtime\n      descriptor: sha256:0000\n",
    );

    assert!(matches!(
        Document::parse(&manifest),
        Err(crate::PlatformGitError::Rejected { .. })
    ));
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
