//! The publication record's wire shape and floors.

use crate::{
    DocumentRevision, PublicationRecord, RecordedDocument, RecordedDocuments, ResourceHistory,
    CONTRACT_VERSION, MINIMUM_READER_CONTRACT,
};

fn record(generation: u64) -> PublicationRecord {
    PublicationRecord::new(
        generation,
        RecordedDocuments {
            tenants: RecordedDocument::of(DocumentRevision::new(3), b"[]\n"),
            data_sources: RecordedDocument::of(DocumentRevision::new(2), b"[]\n"),
            catalog: RecordedDocument::of(DocumentRevision::new(1), b"{}\n"),
        },
        ResourceHistory::default(),
    )
}

#[test]
fn a_record_serialises_every_floor_and_each_document_under_its_wire_name() {
    let json: serde_json::Value = serde_json::from_slice(&record(4).canonical_json().unwrap()).unwrap();

    assert_eq!(json["contract_version"], CONTRACT_VERSION);
    assert_eq!(json["minimum_reader_contract"], MINIMUM_READER_CONTRACT);
    assert_eq!(json["minimum_publisher_contract"], CONTRACT_VERSION);
    assert_eq!(json["generation"], 4);
    assert_eq!(json["documents"]["data-sources"]["revision"], 2);
    assert_eq!(json["documents"]["tenants"]["sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn a_held_record_this_publisher_wrote_parses_back_to_itself() {
    let written = record(7);

    let held = PublicationRecord::parse_held(&written.canonical_json().unwrap()).unwrap();

    assert_eq!(held, written);
}

#[test]
fn the_record_file_name_is_valid_configmap_data() {
    assert!(crate::PUBLICATION_RECORD_FILE
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_')));
}

#[test]
fn a_record_carries_resource_history_under_resources_and_reads_without_it() {
    let tenants = vec![];
    let mut data_sources = vec![];
    let source: crate::DataSourceDocument = serde_json::from_value(serde_json::json!({
        "id": "shared-1", "revision": 4, "connector": "postgres",
        "connection": {"kind": "named", "name": "shared"}, "placement": "shared",
        "residency": {"region": "nz"}, "pool": {}, "capabilities": {}, "labels": {}
    }))
    .unwrap();
    data_sources.push(source);
    let with_history = PublicationRecord::new(
        2,
        record(2).documents().clone(),
        ResourceHistory::default().including(&tenants, &data_sources),
    );
    let json: serde_json::Value = serde_json::from_slice(&with_history.canonical_json().unwrap()).unwrap();
    assert_eq!(json["resources"]["data-sources"]["shared-1"], 4);

    let mut before_history = json;
    before_history.as_object_mut().unwrap().remove("resources");
    let held = PublicationRecord::parse_held(&serde_json::to_vec(&before_history).unwrap()).unwrap();
    assert_eq!(held.resources(), &ResourceHistory::default());
}
