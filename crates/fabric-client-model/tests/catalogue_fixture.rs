//! A catalogue written before the shapes it holds moved to
//! `fabric-component` still reads and re-renders byte for byte.
//!
//! # Why a fixture, and not a round trip of a value built here
//!
//! A round trip proves only that this build agrees with itself. The fixture
//! is text: it was rendered by the build before `ConfigurationField`,
//! `FieldKind` and `ApplicationResource` moved (ADR 0026, "What is built
//! first", slice 1), and every catalogue already in a repository is text
//! like it. Re-rendering it identically is what "moved, byte for byte"
//! means -- a changed field order, a skipped default or a renamed variant
//! would turn every stored catalogue into a diff on its next unrelated save.

use fabric_client_model::catalogue::Catalogue;

const FIXTURE: &str = include_str!("fixtures/catalogue.yaml");

#[test]
fn an_existing_catalogue_re_renders_byte_identically() {
    let catalogue = Catalogue::parse(FIXTURE).unwrap();

    let rendered = catalogue.render().unwrap();

    assert_eq!(rendered, FIXTURE);
}

#[test]
fn the_fixture_exercises_every_field_kind_and_a_declared_resource() {
    let catalogue = Catalogue::parse(FIXTURE).unwrap();
    let draft = &catalogue.applications[0].draft;

    assert_eq!(draft.fields.len(), 7);
    assert_eq!(draft.resources.len(), 2);
    assert_eq!(catalogue.client_fields.len(), 1);
}
