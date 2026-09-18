//! The rendered layout is a cross-repository contract, so it is pinned.

use super::Document;

/// Exactly what render must produce, header and all. Lives beside the
/// tests rather than in the platform repository, for the same reason
/// `components/document_tests.rs`'s own fixture does.
const CANONICAL: &str = include_str!("../../../tests/fixtures/data-sources.yaml");

#[test]
fn the_canonical_document_round_trips_byte_for_byte() {
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
fn a_discriminator_survives_the_round_trip() {
    let document = Document::parse(CANONICAL).expect("parses");
    let declarations = document.into_declarations();

    assert_eq!(declarations.len(), 1);

    let discriminator = declarations[0].discriminator.as_ref().expect("a discriminator");
    assert_eq!(discriminator.column.as_str(), "tenant_key");
}

use fabric_platform_management::DataSourceDeclaration;

fn declaration(id: &str) -> DataSourceDeclaration {
    let text = format!(
        r"
id: {id}
revision: 0
connector: postgres-nz
connection:
  kind: named
  name: shared
placement: dedicated
residency:
  region: nz
"
    );

    serde_norway::from_str(&text).expect("a valid declaration fixture")
}

#[test]
fn document_new_sorts_declarations_regardless_of_input_order() {
    let document = Document::new(String::new(), "lucentroot", &[declaration("b"), declaration("a")]);

    let ids: Vec<String> = document
        .into_declarations()
        .into_iter()
        .map(|declared| declared.id.as_str().to_owned())
        .collect();

    assert_eq!(ids, vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
fn a_file_with_entries_out_of_order_reads_back_sorted_by_id() {
    let text = r"---
schemaVersion: 1
environment: lucentroot
dataSources:
- id: b
  revision: 1
  connector: postgres-nz
  connection:
    kind: named
    name: shared
  placement: dedicated
  residency:
    region: nz
- id: a
  revision: 1
  connector: postgres-nz
  connection:
    kind: named
    name: shared
  placement: dedicated
  residency:
    region: nz
";

    let document = Document::parse(text).expect("parses despite being out of order");
    let ids: Vec<String> = document
        .into_declarations()
        .into_iter()
        .map(|declared| declared.id.as_str().to_owned())
        .collect();

    assert_eq!(ids, vec!["a".to_owned(), "b".to_owned()]);
}
