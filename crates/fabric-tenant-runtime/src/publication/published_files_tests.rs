//! Reading the published files as one generation, or refusing to.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::digest::sha256_hex;
use super::{PublishedFiles, PublishedState, PublishedStateSource as _, READER_CONTRACT};
use crate::testing::{data_source, tenant_binding};
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// A fresh directory under the process temp dir, removed on drop.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fabric-published-files-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(name), bytes).unwrap();
    }

    fn files(&self) -> PublishedFiles {
        PublishedFiles::new(self.0.join("tenants.json"), self.0.join("data-sources.json"))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tenants_bytes(data_source: &str) -> Vec<u8> {
    serde_json::to_vec_pretty(&vec![tenant_binding("acme", 1, data_source)]).unwrap()
}

fn data_sources_bytes(ids: &[&str]) -> Vec<u8> {
    let sources: Vec<DataSource> = ids.iter().map(|id| data_source(id, 1)).collect();
    serde_json::to_vec_pretty(&sources).unwrap()
}

fn record(generation: u64, minimum_reader_contract: u32, tenants: &[u8], data_sources: &[u8]) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "contract_version": 1,
        "minimum_reader_contract": minimum_reader_contract,
        "minimum_publisher_contract": 1,
        "generation": generation,
        "documents": {
            "tenants": { "revision": generation, "sha256": sha256_hex(tenants) },
            "data-sources": { "revision": generation, "sha256": sha256_hex(data_sources) },
            "catalog": { "revision": 1, "sha256": sha256_hex(b"{}") },
        },
    }))
    .unwrap()
}

/// Publishes one complete generation into `dir`.
fn publish(dir: &Dir, generation: u64, data_source: &str) -> (Vec<u8>, Vec<u8>) {
    let tenants = tenants_bytes(data_source);
    let data_sources = data_sources_bytes(&[data_source]);
    dir.write("data-sources.json", &data_sources);
    dir.write("tenants.json", &tenants);
    dir.write(
        "publication.json",
        &record(generation, READER_CONTRACT, &tenants, &data_sources),
    );
    (tenants, data_sources)
}

fn bound_to(tenants: &[TenantRuntimeBinding]) -> String {
    let binding = tenants.first().unwrap();
    binding.data.values().next().unwrap().data_source.to_string()
}

#[tokio::test]
async fn documents_matching_the_record_are_read_as_one_generation() {
    let dir = Dir::new();
    publish(&dir, 4, "pg-1");

    let loaded = dir.files().load().await.unwrap();

    let PublishedState::Generation {
        generation,
        record_sha256,
        origin,
        data_sources,
        tenants,
    } = loaded
    else {
        panic!("expected a complete generation, got {loaded:?}");
    };
    assert_eq!(generation, 4);
    let held_record = std::fs::read(dir.path().join(super::record::PUBLICATION_RECORD_FILE)).unwrap();
    assert_eq!(record_sha256, super::digest::sha256_digest(&held_record));
    assert_eq!(origin, super::GenerationOrigin::Published);
    assert_eq!(data_sources.len(), 1);
    assert_eq!(bound_to(&tenants), "pg-1");
}

#[tokio::test]
async fn data_sources_newer_than_the_record_are_an_incomplete_generation() {
    let dir = Dir::new();
    publish(&dir, 1, "pg-1");
    dir.write("data-sources.json", &data_sources_bytes(&["pg-1", "pg-2"]));

    let refused = dir.files().load().await.unwrap_err();

    assert!(
        matches!(
            refused,
            SourceError::IncompleteGeneration {
                generation: 1,
                document: "data-sources",
                ..
            }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn tenants_newer_than_the_record_are_an_incomplete_generation() {
    let dir = Dir::new();
    publish(&dir, 1, "pg-1");
    dir.write("tenants.json", &tenants_bytes("pg-2"));

    let refused = dir.files().load().await.unwrap_err();

    assert!(
        matches!(
            refused,
            SourceError::IncompleteGeneration {
                document: "tenants",
                ..
            }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_record_newer_than_its_documents_is_an_incomplete_generation() {
    let dir = Dir::new();
    let (tenants, _) = publish(&dir, 1, "pg-1");
    dir.write(
        "publication.json",
        &record(
            2,
            READER_CONTRACT,
            &tenants,
            &data_sources_bytes(&["pg-1", "pg-2"]),
        ),
    );

    let refused = dir.files().load().await.unwrap_err();

    assert!(
        matches!(refused, SourceError::IncompleteGeneration { generation: 2, .. }),
        "{refused}"
    );
}

#[tokio::test]
async fn a_record_that_requires_a_newer_reader_is_refused_by_its_floor() {
    let dir = Dir::new();
    let (tenants, data_sources) = publish(&dir, 1, "pg-1");
    dir.write(
        "publication.json",
        &record(2, READER_CONTRACT + 1, &tenants, &data_sources),
    );

    let refused = dir.files().load().await.unwrap_err();

    assert!(
        matches!(
            refused,
            SourceError::UnsupportedContract { required, supported, .. }
                if required == READER_CONTRACT + 1 && supported == READER_CONTRACT
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_record_with_fields_this_reader_does_not_know_is_read_within_its_floor() {
    let dir = Dir::new();
    let (tenants, data_sources) = publish(&dir, 3, "pg-1");
    let mut newer: serde_json::Value =
        serde_json::from_slice(&record(3, READER_CONTRACT, &tenants, &data_sources)).unwrap();
    newer["contract_version"] = serde_json::json!(READER_CONTRACT + 1);
    newer["documents"]["audit"] = serde_json::json!({ "revision": 1, "sha256": "00" });
    newer["generation_started_by"] = serde_json::json!("a field a later publisher adds");
    dir.write("publication.json", &serde_json::to_vec(&newer).unwrap());

    let loaded = dir.files().load().await.unwrap();

    assert!(
        matches!(loaded, PublishedState::Generation { generation: 3, .. }),
        "{loaded:?}"
    );
}

#[tokio::test]
async fn without_a_record_each_document_is_read_on_its_own() {
    let dir = Dir::new();
    dir.write("tenants.json", &tenants_bytes("pg-1"));
    dir.write("data-sources.json", b"{ torn");

    let loaded = dir.files().load().await.unwrap();

    let PublishedState::Unrecorded {
        data_sources,
        tenants,
    } = loaded
    else {
        panic!("expected an unrecorded set, got {loaded:?}");
    };
    assert!(matches!(data_sources, Err(SourceError::Malformed { .. })));
    assert_eq!(bound_to(&tenants.unwrap()), "pg-1");
}

#[tokio::test]
async fn a_record_that_does_not_parse_is_malformed_rather_than_ignored() {
    let dir = Dir::new();
    publish(&dir, 1, "pg-1");
    dir.write("publication.json", b"{ torn");

    let refused = dir.files().load().await.unwrap_err();

    assert!(matches!(refused, SourceError::Malformed { .. }), "{refused}");
    assert!(dir.path().join("publication.json").exists());
}
