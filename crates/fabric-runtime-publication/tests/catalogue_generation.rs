//! G8: the catalogue a runtime starts on is the one the publication record
//! names, so a process never pairs a catalogue from one generation with
//! another generation's tenants. Driven through the real
//! `FilesystemRuntimePublication` and the reader's own
//! `startup_catalogue`, which `fabric-api` loads its catalogue with.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_core::{BindingRevision, LogicalResourceName, TenantId};
use fabric_runtime_publication::{
    CatalogDocument, DocumentInput, DocumentRevision, FilesystemRuntimePublication, RuntimePublication as _,
    RuntimeSnapshot, CATALOG_FILE, PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{
    build_published_runtime, startup_catalogue, PublishedFiles, PublishedState, PublishedStateSource,
    SourceError,
};

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

    let bytes = startup_catalogue(None, dir.tenants_path(), dir.catalog_path())
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
    let refused = startup_catalogue(None, dir.tenants_path(), dir.catalog_path())
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
    let refused = startup_catalogue(None, dir.tenants_path(), dir.catalog_path())
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

    let bytes = startup_catalogue(None, dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap();

    assert_eq!(bytes, dir.read_raw(CATALOG_FILE));
}

/// Generation 2 of the base set: tenants at revision 2, and the catalogue's
/// one resource renamed from `articles` to `private_articles`.
fn renamed(revision: u64) -> RuntimeSnapshot {
    let mut snapshot = support::base_snapshot(revision);
    let resources = support::articles_catalog()
        .resources()
        .map(|(_, definition)| {
            (
                LogicalResourceName::try_new("private_articles").unwrap(),
                definition.clone(),
            )
        })
        .collect();
    snapshot.catalog = DocumentInput::new(DocumentRevision::new(revision), CatalogDocument::new(resources));
    snapshot
}

/// `PublishedFiles`, with a publication landing the moment its start-up
/// read returns -- after the resolver's generation is read, before anything
/// else is: the window between priming and loading the catalogue.
struct PublishesAfterStartupRead {
    inner: PublishedFiles,
    publisher: FilesystemRuntimePublication,
    next: Mutex<Option<RuntimeSnapshot>>,
}

#[async_trait]
impl PublishedStateSource for PublishesAfterStartupRead {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        self.inner.load().await
    }

    async fn load_at_start(&self) -> Result<(PublishedState, Option<Vec<u8>>), SourceError> {
        let read = self.inner.load_at_start().await;
        let next = self.next.lock().unwrap().take();
        if let Some(next) = next {
            self.publisher.publish(&next).await.unwrap();
        }
        read
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

#[tokio::test]
async fn a_publication_between_priming_and_the_catalogue_read_does_not_reach_the_catalogue() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let first_catalogue = dir.read_raw(CATALOG_FILE);
    let source = PublishesAfterStartupRead {
        inner: PublishedFiles::new(dir.tenants_path(), dir.data_sources_path())
            .with_catalogue(dir.catalog_path()),
        publisher: dir.publisher(),
        next: Mutex::new(Some(renamed(2))),
    };

    // fabric-api's startup: prime the runtime, then load the catalogue.
    let runtime = build_published_runtime(&support::runtime_config(), Arc::new(source))
        .await
        .unwrap();
    let catalogue = startup_catalogue(runtime.catalogue.clone(), dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap();

    let acme = runtime
        .resolver
        .resolve_tenant(&TenantId::try_new("acme").unwrap())
        .unwrap();
    assert_eq!(acme.revision, BindingRevision::new(1));
    assert_eq!(catalogue, first_catalogue, "the primed generation's catalogue");
    let catalogue: serde_json::Value = serde_json::from_slice(&catalogue).unwrap();
    assert!(catalogue.get("articles").is_some(), "{catalogue}");
    assert!(catalogue.get("private_articles").is_none(), "{catalogue}");

    // The race is real: a catalogue read now, against the record now held,
    // is generation 2's -- valid, and not the generation the resolver holds.
    let read_after_priming = startup_catalogue(None, dir.tenants_path(), dir.catalog_path())
        .await
        .unwrap();
    assert_eq!(read_after_priming, dir.read_raw(CATALOG_FILE));
    assert_ne!(read_after_priming, first_catalogue);
}

#[tokio::test]
async fn a_catalogue_from_another_generation_than_the_documents_fails_the_prime() {
    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let first = dir.capture(&[CATALOG_FILE]);
    dir.publisher().publish(&renamed(2)).await.unwrap();
    dir.restore(&first);

    let refused = build_published_runtime(
        &support::runtime_config(),
        Arc::new(
            PublishedFiles::new(dir.tenants_path(), dir.data_sources_path())
                .with_catalogue(dir.catalog_path()),
        ),
    )
    .await
    .err()
    .expect("a catalogue the record does not name is refused at start");

    assert!(refused.contains("catalog"), "{refused}");
}
