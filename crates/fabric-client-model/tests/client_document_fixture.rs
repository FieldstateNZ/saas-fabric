//! A client document written before the shapes it holds moved to
//! `fabric-component` still reads and re-renders byte for byte.
//!
//! # Why a client document as well as the catalogue
//!
//! A client document's `spec.product` stores frozen copies of every release
//! a client is assigned, each with its own `fields` and `resources`, and they
//! are serialized by a path of their own: `ClientDocument::product` reads
//! them and `with_product` writes them back, neither through
//! `Catalogue::render`. The catalogue's fixture does not reach that path.
//! ADR 0026, "What is built first", slice 1, asks for both.
//!
//! # Why a fixture
//!
//! As in `catalogue_fixture.rs`: `fixtures/client.yaml` was rendered by the
//! build before `ConfigurationField`, `FieldKind` and `ApplicationResource`
//! moved, and re-rendered identically by that build. Its release holds a
//! field of every kind and two resources, one restricting its queryable
//! fields and one keyed on a field other than `id`.

use fabric_client_model::catalogue::{ClientProductRequest, FieldKind};
use fabric_client_model::ClientDocument;

const FIXTURE: &str = include_str!("fixtures/client.yaml");

#[test]
fn an_existing_client_document_re_renders_byte_identically() {
    let document = ClientDocument::parse(FIXTURE).unwrap();

    let rendered = document.render().unwrap();

    assert_eq!(rendered, FIXTURE);
}

#[test]
fn its_frozen_release_is_read_and_written_back_byte_identically() {
    let document = ClientDocument::parse(FIXTURE).unwrap();
    let product = document.product().unwrap();
    let client = document.client();
    let request = ClientProductRequest {
        display_name: client.display_name.clone(),
        hosts: client.hosts.clone(),
        legal_name: product.legal_name.clone(),
        region: product.region.clone(),
        timezone: product.timezone.clone(),
        configuration: product.configuration.clone(),
        applications: vec![],
    };

    let rendered = document
        .with_product(&request, &product)
        .unwrap()
        .render()
        .unwrap();

    assert_eq!(rendered, FIXTURE);
}

#[test]
fn the_fixture_exercises_every_field_kind_and_both_resources() {
    let product = ClientDocument::parse(FIXTURE).unwrap().product().unwrap();
    let definition = &product.applications[0].release.definition;

    let kinds: Vec<FieldKind> = definition.fields.iter().map(|field| field.kind).collect();

    assert_eq!(
        kinds,
        [
            FieldKind::Text,
            FieldKind::Number,
            FieldKind::Boolean,
            FieldKind::Choice,
            FieldKind::Hostname,
            FieldKind::Identifier,
            FieldKind::Timezone,
        ]
    );
    assert_eq!(definition.resources.len(), 2);
    assert!(!definition.resources[0].queryable_fields.is_empty());
    assert_eq!(definition.resources[1].key_field.as_str(), "scheduleId");
}
