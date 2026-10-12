//! The published shape is held to `published-shapes.json`, and the floor it
//! was recorded at is the one this publisher writes (gap report G6).
//!
//! A reader's document types deny unknown fields, so a document whose shape
//! changed is refused by every older reader. The publication record's
//! `minimum_reader_contract` is how a publisher says so: readers below it
//! keep their last complete generation instead of refusing the shape. This
//! suite fails when the shape this publisher writes differs from the lock.
//! `scripts/check_reader_floor.py` fails CI when the lock's shape changed
//! from the base commit's but its floor did not rise.
//!
//! To record a deliberate shape change: raise `MINIMUM_READER_CONTRACT`,
//! `compatibility.yaml`'s `minimumReaderContract` and, if needed,
//! `fabric-tenant-runtime`'s `READER_CONTRACT`, then run this suite with
//! `FABRIC_WRITE_PUBLISHED_SHAPES=1`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod shapes;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fabric_data_api::ResourceCatalog;
use fabric_runtime_publication::MINIMUM_READER_CONTRACT;
use fabric_tenant_runtime::{DataSource, TenantRuntimeBinding, READER_CONTRACT};
use serde_json::{json, Value};

const WRITE: &str = "FABRIC_WRITE_PUBLISHED_SHAPES";

fn lock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("published-shapes.json")
}

fn lock() -> Value {
    serde_json::from_slice(&std::fs::read(lock_path()).unwrap()).unwrap()
}

fn locked_shape(lock: &Value) -> BTreeMap<String, BTreeSet<String>> {
    serde_json::from_value(lock["shapes"].clone()).unwrap()
}

fn rendered(shape: &BTreeMap<String, BTreeSet<String>>) -> Vec<u8> {
    let lock = json!({
        "about": "The published shape of tenants.json, data-sources.json, catalog.json and the publication.json keys readers read, and the minimum reader contract it was recorded at. Written by `FABRIC_WRITE_PUBLISHED_SHAPES=1 cargo test -p fabric-runtime-publication --test published_shapes`. scripts/check_reader_floor.py fails CI when shapes change without minimumReaderContract rising.",
        "minimumReaderContract": MINIMUM_READER_CONTRACT,
        "shapes": shape,
    });
    let mut bytes = serde_json::to_vec_pretty(&lock).unwrap();
    bytes.push(b'\n');
    bytes
}

fn difference(
    locked: &BTreeMap<String, BTreeSet<String>>,
    written: &BTreeMap<String, BTreeSet<String>>,
) -> String {
    let empty = BTreeSet::new();
    let documents: BTreeSet<&String> = locked.keys().chain(written.keys()).collect();
    let mut lines = Vec::new();
    for document in documents {
        let before = locked.get(document).unwrap_or(&empty);
        let after = written.get(document).unwrap_or(&empty);
        lines.extend(
            before
                .difference(after)
                .map(|line| format!("- {document} {line}")),
        );
        lines.extend(
            after
                .difference(before)
                .map(|line| format!("+ {document} {line}")),
        );
    }
    lines.join("\n")
}

#[test]
fn the_lock_is_the_shape_this_publisher_writes() {
    let written = shapes::published_shape();
    if std::env::var_os(WRITE).is_some() {
        std::fs::write(lock_path(), rendered(&written)).unwrap();
        return;
    }
    let locked = locked_shape(&lock());
    assert!(
        locked == written,
        "the published shape differs from published-shapes.json:\n{}\n\n\
         Older readers refuse a document whose shape they do not know. Raise \
         MINIMUM_READER_CONTRACT and compatibility.yaml's minimumReaderContract, \
         then rerun with {WRITE}=1 to record the shape at the new floor.",
        difference(&locked, &written)
    );
}

#[test]
fn the_lock_is_byte_for_byte_what_this_suite_writes() {
    let bytes = std::fs::read(lock_path()).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        String::from_utf8(rendered(&locked_shape(&lock()))).unwrap(),
        "regenerate published-shapes.json with {WRITE}=1 instead of editing it"
    );
}

#[test]
fn the_lock_is_recorded_at_the_floor_this_publisher_writes() {
    assert_eq!(
        lock()["minimumReaderContract"].as_u64(),
        Some(u64::from(MINIMUM_READER_CONTRACT))
    );
}

#[test]
fn this_sources_reader_serves_the_floor_this_publisher_writes() {
    const { assert!(READER_CONTRACT >= MINIMUM_READER_CONTRACT) };
}

/// The specimens are documents this source's own reader accepts, so the
/// shape is one a reader at the floor actually reads.
#[test]
fn the_specimens_are_read_by_this_sources_reader() {
    let published = shapes::published();
    serde_json::from_slice::<Vec<TenantRuntimeBinding>>(&published["tenants.json"]).unwrap();
    serde_json::from_slice::<Vec<DataSource>>(&published["data-sources.json"]).unwrap();
    serde_json::from_slice::<ResourceCatalog>(&published["catalog.json"]).unwrap();
}

/// The walker reports what the matrix says a reader refuses: a field added
/// or removed, a variant added, and a field becoming optional.
#[test]
fn each_kind_of_shape_change_changes_the_shape() {
    use shapes::walk::{shape, Rules};
    let rules = Rules {
        maps: &["$.labels"],
        tagged: &[("$.connection", "kind")],
        enums: &["$.placement"],
    };
    let base = shape(
        &json!({ "id": "a", "placement": "shared", "labels": { "x": "y" }, "connection": { "kind": "named", "name": "n" } }),
        &rules,
    );
    for changed in [
        json!({ "id": "a", "tier": 1, "placement": "shared", "labels": { "x": "y" }, "connection": { "kind": "named", "name": "n" } }),
        json!({ "placement": "shared", "labels": { "x": "y" }, "connection": { "kind": "named", "name": "n" } }),
        json!({ "id": "a", "placement": "archive", "labels": { "x": "y" }, "connection": { "kind": "named", "name": "n" } }),
        json!({ "id": "a", "placement": "shared", "labels": { "x": "y" }, "connection": { "kind": "vault", "name": "n" } }),
        json!({ "id": "a", "placement": "shared", "labels": { "x": "y" }, "connection": { "kind": "named", "name": 1 } }),
    ] {
        assert_ne!(shape(&changed, &rules), base, "{changed}");
    }
    let unchanged = json!({ "id": "b", "placement": "shared", "labels": { "other": "z" }, "connection": { "kind": "named", "name": "m" } });
    assert_eq!(shape(&unchanged, &rules), base);

    let mut optional = shape(
        &json!([{ "id": "a" }, {}]),
        &Rules {
            maps: &[],
            tagged: &[],
            enums: &[],
        },
    )
    .into_iter();
    assert!(optional.any(|line| line == "$[].id: may be absent"));
}
