//! The published shape (G6): what a reader of the three documents and the
//! publication record depends on, read from the bytes this publisher writes
//! for [`specimens`].

pub mod specimens;
pub mod variants;
pub mod walk;

use std::collections::{BTreeMap, BTreeSet};

use fabric_runtime_publication::{
    data_sources_canonical_json, tenants_canonical_json, DocumentRevision, PublicationRecord,
    RecordedDocument, RecordedDocuments, ResourceHistory,
};
use serde_json::Value;
use walk::{shape, Rules};

/// The record keys a reader reads (`fabric-tenant-runtime`'s `ReaderRecord`).
/// The reader's record type does not deny unknown fields, so a key outside
/// these is free to add without raising the floor.
const READER_RECORD_PATHS: &[&str] = &[
    "$",
    "$.minimum_reader_contract",
    "$.generation",
    "$.documents",
    "$.documents.tenants",
    "$.documents.tenants.sha256",
    "$.documents.data-sources",
    "$.documents.data-sources.sha256",
    "$.documents.catalog",
    "$.documents.catalog.sha256",
];

const TENANTS: Rules = Rules {
    maps: &["$[].data", "$[].features", "$[].storage"],
    tagged: &[("$[].data.*.isolation", "kind")],
    enums: &[],
};

const DATA_SOURCES: Rules = Rules {
    maps: &["$[].labels"],
    tagged: &[("$[].connection", "kind")],
    enums: &["$[].placement"],
};

const CATALOG: Rules = Rules {
    maps: &["$"],
    tagged: &[],
    enums: &["$.*.operations[]"],
};

const RECORD: Rules = Rules {
    maps: &[],
    tagged: &[],
    enums: &[],
};

fn parsed(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

/// The canonical bytes this publisher writes for each specimen document,
/// and a record naming them, by file name.
pub fn published() -> BTreeMap<&'static str, Vec<u8>> {
    let tenants = tenants_canonical_json(&specimens::tenants()).unwrap();
    let data_sources = data_sources_canonical_json(&specimens::data_sources()).unwrap();
    let catalog = specimens::catalog().canonical_json().unwrap();
    let documents = RecordedDocuments {
        tenants: RecordedDocument::of(DocumentRevision::new(1), &tenants),
        data_sources: RecordedDocument::of(DocumentRevision::new(1), &data_sources),
        catalog: RecordedDocument::of(DocumentRevision::new(1), &catalog),
    };
    let record = PublicationRecord::new(1, documents, ResourceHistory::default())
        .canonical_json()
        .unwrap();
    BTreeMap::from([
        ("tenants.json", tenants),
        ("data-sources.json", data_sources),
        ("catalog.json", catalog),
        ("publication.json", record),
    ])
}

/// The published shape, by file name.
pub fn published_shape() -> BTreeMap<String, BTreeSet<String>> {
    let published = published();
    let record = shape(&parsed(&published["publication.json"]), &RECORD)
        .into_iter()
        .filter(|line| {
            let (path, _) = line.split_once(": ").unwrap();
            READER_RECORD_PATHS.contains(&path)
        })
        .collect();
    BTreeMap::from([
        (
            "tenants.json".to_owned(),
            shape(&parsed(&published["tenants.json"]), &TENANTS),
        ),
        (
            "data-sources.json".to_owned(),
            shape(&parsed(&published["data-sources.json"]), &DATA_SOURCES),
        ),
        (
            "catalog.json".to_owned(),
            shape(&parsed(&published["catalog.json"]), &CATALOG),
        ),
        ("publication.json".to_owned(), record),
    ])
}
