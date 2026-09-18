//! The rendered layout is a cross-repository contract, so it is pinned.

use super::Document;

/// Exactly what render must produce, header and all. Lives beside the
/// tests rather than in the platform repository, for the same reason
/// `data_sources/document_tests.rs`'s own fixture does.
const CANONICAL: &str = include_str!("../../../tests/fixtures/placements.yaml");

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
fn a_discriminator_isolation_survives_the_round_trip() {
    let document = Document::parse(CANONICAL).expect("parses");
    let placements = document.into_placements();

    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].tenant.as_str(), "acme");
    assert_eq!(placements[0].data_source.as_str(), "shared-postgres-nz-01");
}

fn placement(tenant: &str, logical: &str) -> fabric_platform_management::PlacementRecord {
    let text = format!(
        r"
tenant: {tenant}
logical: {logical}
data_source: shared-postgres-nz-01
isolation:
  kind: discriminator
  column: tenant_key
  value: {tenant}
placed_at: 2026-09-18T02:14:00Z
"
    );

    serde_norway::from_str(&text).expect("a valid placement fixture")
}

#[test]
fn document_new_sorts_by_tenant_then_logical_regardless_of_input_order() {
    let document = Document::new(
        String::new(),
        "lucentroot",
        &[placement("initech", "primary"), placement("acme", "primary")],
    );

    let tenants: Vec<String> = document
        .into_placements()
        .into_iter()
        .map(|placed| placed.tenant.as_str().to_owned())
        .collect();

    assert_eq!(tenants, vec!["acme".to_owned(), "initech".to_owned()]);
}

#[test]
fn a_file_with_entries_out_of_order_reads_back_sorted() {
    let text = r"---
schemaVersion: 1
environment: lucentroot
placements:
- tenant: initech
  logical: primary
  data_source: shared-postgres-nz-01
  isolation:
    kind: discriminator
    column: tenant_key
    value: initech
  placed_at: 2026-09-18T02:14:00Z
- tenant: acme
  logical: primary
  data_source: shared-postgres-nz-01
  isolation:
    kind: discriminator
    column: tenant_key
    value: acme
  placed_at: 2026-09-18T02:14:00Z
";

    let document = Document::parse(text).expect("parses despite being out of order");
    let tenants: Vec<String> = document
        .into_placements()
        .into_iter()
        .map(|placed| placed.tenant.as_str().to_owned())
        .collect();

    assert_eq!(tenants, vec!["acme".to_owned(), "initech".to_owned()]);
}
