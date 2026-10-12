//! Reading the published files as one generation, or refusing to.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::digest::sha256_hex;
use super::{
    CatalogueDecoder, DecodedCatalogue, PublishedFiles, PublishedState, PublishedStateSource as _,
    READER_CONTRACT,
};
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
        PublishedFiles::new(
            self.0.join("tenants.json"),
            self.0.join("data-sources.json"),
            self.0.join("catalog.json"),
            Arc::new(Names),
        )
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

/// The tests' stand-in for the host's decoder: a JSON object naming the
/// catalogue, decoded as that name.
struct Names;

impl CatalogueDecoder for Names {
    fn decode(&self, catalogue: &[u8]) -> Result<DecodedCatalogue, String> {
        let value: serde_json::Value =
            serde_json::from_slice(catalogue).map_err(|error| error.to_string())?;
        value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(|name| DecodedCatalogue::new(name.to_owned()))
            .ok_or_else(|| "not a catalogue".to_owned())
    }
}

fn catalogue_bytes(generation: u64) -> Vec<u8> {
    format!("{{\"name\":\"catalogue {generation}\"}}").into_bytes()
}

fn named(catalogue: &DecodedCatalogue) -> &str {
    catalogue.get::<String>().unwrap()
}

fn tenants_bytes(data_source: &str) -> Vec<u8> {
    serde_json::to_vec_pretty(&vec![tenant_binding("acme", 1, data_source)]).unwrap()
}

fn data_sources_bytes(ids: &[&str]) -> Vec<u8> {
    let sources: Vec<DataSource> = ids.iter().map(|id| data_source(id, 1)).collect();
    serde_json::to_vec_pretty(&sources).unwrap()
}

fn record(generation: u64, minimum_reader_contract: u32, tenants: &[u8], data_sources: &[u8]) -> Vec<u8> {
    record_with_catalogue(
        generation,
        minimum_reader_contract,
        (tenants, data_sources, &catalogue_bytes(generation)),
    )
}

fn record_with_catalogue(
    generation: u64,
    minimum_reader_contract: u32,
    (tenants, data_sources, catalogue): (&[u8], &[u8], &[u8]),
) -> Vec<u8> {
    serde_json::to_vec_pretty(&serde_json::json!({
        "contract_version": 1,
        "minimum_reader_contract": minimum_reader_contract,
        "minimum_publisher_contract": 1,
        "generation": generation,
        "documents": {
            "tenants": { "revision": generation, "sha256": sha256_hex(tenants) },
            "data-sources": { "revision": generation, "sha256": sha256_hex(data_sources) },
            "catalog": { "revision": generation, "sha256": sha256_hex(catalogue) },
        },
    }))
    .unwrap()
}

/// Publishes one complete generation into `dir`.
fn publish(dir: &Dir, generation: u64, data_source: &str) -> (Vec<u8>, Vec<u8>) {
    let tenants = tenants_bytes(data_source);
    let data_sources = data_sources_bytes(&[data_source]);
    dir.write("data-sources.json", &data_sources);
    dir.write("catalog.json", &catalogue_bytes(generation));
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
        catalogue,
    } = loaded
    else {
        panic!("expected a complete generation, got {loaded:?}");
    };
    assert_eq!(generation, 4);
    assert_eq!(named(&catalogue), "catalogue 4");
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
async fn without_a_record_all_three_documents_are_read_and_decoded_together() {
    let dir = Dir::new();
    dir.write("tenants.json", &tenants_bytes("pg-1"));
    dir.write("data-sources.json", &data_sources_bytes(&["pg-1"]));
    dir.write("catalog.json", &catalogue_bytes(7));

    let loaded = dir.files().load().await.unwrap();

    let PublishedState::Unrecorded {
        data_sources,
        tenants,
        catalogue,
    } = loaded
    else {
        panic!("expected an unrecorded set, got {loaded:?}");
    };
    assert_eq!(data_sources.len(), 1);
    assert_eq!(bound_to(&tenants), "pg-1");
    assert_eq!(named(&catalogue), "catalogue 7");
}

#[tokio::test]
async fn without_a_record_any_document_that_cannot_be_read_or_decoded_holds_the_whole_view() {
    let cases: [(&str, &[u8]); 3] = [
        ("data-sources.json", b"{ torn"),
        ("tenants.json", b"{ torn"),
        ("catalog.json", b"{\"not\": \"a catalogue\"}"),
    ];
    for (file, broken) in cases {
        let dir = Dir::new();
        dir.write("tenants.json", &tenants_bytes("pg-1"));
        dir.write("data-sources.json", &data_sources_bytes(&["pg-1"]));
        dir.write("catalog.json", &catalogue_bytes(7));
        dir.write(file, broken);

        let refused = dir.files().load().await.unwrap_err();

        assert!(
            matches!(refused, SourceError::Malformed { .. }),
            "{file}: {refused}"
        );
    }
}

#[tokio::test]
async fn a_catalogue_that_lags_its_record_is_an_incomplete_generation() {
    let dir = Dir::new();
    publish(&dir, 2, "pg-1");
    dir.write("catalog.json", &catalogue_bytes(1));

    let refused = dir.files().load().await.unwrap_err();

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
async fn a_catalogue_that_matches_its_digest_but_does_not_decode_is_unusable() {
    let dir = Dir::new();
    let (tenants, data_sources) = publish(&dir, 2, "pg-1");
    let undecodable = b"{\"not\": \"a catalogue\"}";
    dir.write("catalog.json", undecodable);
    dir.write(
        "publication.json",
        &record_with_catalogue(2, READER_CONTRACT, (&tenants, &data_sources, undecodable)),
    );

    let refused = dir.files().load().await.unwrap_err();

    assert!(
        matches!(
            &refused,
            SourceError::Unusable { generation: 2, document: "catalog", reason, .. } if reason == "not a catalogue"
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_missing_catalogue_is_unreadable() {
    let dir = Dir::new();
    publish(&dir, 2, "pg-1");
    std::fs::remove_file(dir.path().join("catalog.json")).unwrap();

    let refused = dir.files().load().await.unwrap_err();

    assert!(matches!(refused, SourceError::Unreadable { .. }), "{refused}");
}

#[tokio::test]
async fn once_a_recorded_generation_is_installed_losing_the_record_holds_it() {
    let dir = Dir::new();
    publish(&dir, 3, "pg-1");
    let files = dir.files();
    let PublishedState::Generation {
        generation,
        record_sha256,
        ..
    } = files.load().await.unwrap()
    else {
        panic!("expected a complete generation");
    };
    std::fs::remove_file(dir.path().join("publication.json")).unwrap();
    assert!(
        matches!(files.load().await.unwrap(), PublishedState::Unrecorded { .. }),
        "loaded but never installed: nothing recorded is held yet"
    );

    files.installed(generation, &record_sha256).await;
    let refused = files.load().await.unwrap_err();

    assert!(
        matches!(&refused, SourceError::Regressed { held: 3, found, .. } if found == "no publication record"),
        "{refused}"
    );
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
