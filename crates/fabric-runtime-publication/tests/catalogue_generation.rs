//! G8: the catalogue a runtime starts on is the one the publication record
//! names, so a process never pairs a catalogue from one generation with
//! another generation's tenants. Driven through the real
//! `FilesystemRuntimePublication` and the reader's own
//! `read_recorded_catalogue`, which `fabric-api` loads its catalogue with.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_core::LogicalResourceName;
use fabric_runtime_publication::{
    CatalogDocument, DocumentInput, DocumentRevision, RuntimePublication as _, CATALOG_FILE,
    PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{read_recorded_catalogue, SourceError};

/// The base snapshot at `revision`, with the catalogue's one resource also
/// published as `posts` -- a second catalogue, different bytes.
fn with_a_second_resource(revision: u64) -> fabric_runtime_publication::RuntimeSnapshot {
    let mut snapshot = support::base_snapshot(revision);
    let mut resources: std::collections::BTreeMap<_, _> = support::articles_catalog()
        .resources()
        .map(|(name, definition)| (name.clone(), definition.clone()))
        .collect();
    let articles = resources.values().next().unwrap().clone();
    resources.insert(LogicalResourceName::try_new("posts").unwrap(), articles);
    snapshot.catalog = DocumentInput::new(DocumentRevision::new(revision), CatalogDocument::new(resources));
    snapshot
}

#[tokio::test]
async fn the_catalogue_the_record_names_is_read() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();

    let bytes = read_recorded_catalogue(dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap();

    assert_eq!(bytes, dir.read_raw(CATALOG_FILE));
}

#[tokio::test]
async fn a_catalogue_from_another_generation_is_refused_at_start() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let first = dir.capture(&[CATALOG_FILE]);
    dir.publisher().publish(&with_a_second_resource(2)).await.unwrap();

    // The catalogue volume has not been projected yet; the tenants volume,
    // and the record in it, have.
    dir.restore(&first);
    let refused = read_recorded_catalogue(dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap_err();

    assert!(
        matches!(
            refused,
            SourceError::IncompleteGeneration {
                generation: 2,
                document: "catalog",
                ..
            }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_catalogue_ahead_of_the_record_is_refused_at_start() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let first = dir.capture(&[PUBLICATION_RECORD_FILE]);
    dir.publisher().publish(&with_a_second_resource(2)).await.unwrap();

    dir.restore(&first);
    let refused = read_recorded_catalogue(dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap_err();

    assert!(
        matches!(
            refused,
            SourceError::IncompleteGeneration {
                generation: 1,
                document: "catalog",
                ..
            }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn with_no_record_the_catalogue_is_read_as_it_stands() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    dir.remove_raw(PUBLICATION_RECORD_FILE);

    let bytes = read_recorded_catalogue(dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap();

    assert_eq!(bytes, dir.read_raw(CATALOG_FILE));
}
