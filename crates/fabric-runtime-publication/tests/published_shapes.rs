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

/// Every variant a derive knows reaches the specimens, with no list to keep
/// beside it: a variant added to an enum is enumerated, and so changes the
/// shape, without any specimen being edited.
#[test]
fn every_variant_comes_from_the_enums_own_derive() {
    use shapes::variants::{every_tagged, every_unit};

    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Placement {
        Shared,
        HighAvailability,
        Archive,
    }
    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Selector {
        Default {},
        Vault { path: String, role: String },
    }
    assert_eq!(
        every_unit::<Placement>(),
        [Placement::Shared, Placement::HighAvailability, Placement::Archive]
    );

    assert_eq!(
        every_tagged::<Selector>("kind"),
        [
            Selector::Default {},
            Selector::Vault {
                path: "specimen".to_owned(),
                role: "specimen".to_owned()
            }
        ]
    );
}

/// The specimens carry every variant of every published enum: the shape
/// names every distinct output each derive writes. A deserialize-only alias
/// is enumerated too, but writes its variant's own name, so it adds nothing.
#[test]
fn the_shape_names_every_variant_each_derive_knows() {
    use std::collections::BTreeSet;

    use fabric_runtime_publication::{
        ConnectionSelectorDocument, IsolationModelDocument, PlacementClassDocument,
    };
    use shapes::variants::{every_tagged, every_unit};

    fn written<T: serde::Serialize>(variants: &[T], tag: Option<&str>) -> BTreeSet<String> {
        variants
            .iter()
            .map(|variant| {
                let value = serde_json::to_value(variant).unwrap();
                match tag {
                    Some(tag) => value[tag].as_str().unwrap().to_owned(),
                    None => value.as_str().unwrap().to_owned(),
                }
            })
            .collect()
    }
    let shape = shapes::published_shape();
    let named = |document: &str, prefix: &str, suffix: &str| -> BTreeSet<String> {
        shape[document]
            .iter()
            .filter_map(|line| line.strip_prefix(prefix)?.strip_suffix(suffix).map(str::to_owned))
            .filter(|name| !name.contains(['.', '<', '>', ':']))
            .collect()
    };

    assert_eq!(
        named("tenants.json", "$[].data.*.isolation<kind=", ">: object"),
        written(&every_tagged::<IsolationModelDocument>("kind"), Some("kind"))
    );
    assert_eq!(
        named("data-sources.json", "$[].connection<kind=", ">: object"),
        written(&every_tagged::<ConnectionSelectorDocument>("kind"), Some("kind"))
    );
    assert_eq!(
        named("data-sources.json", "$[].placement: one of \"", "\""),
        written(&every_unit::<PlacementClassDocument>(), None)
    );
    assert_eq!(
        named("catalog.json", "$.*.operations[]: one of \"", "\""),
        written(&every_unit::<fabric_core::OperationKind>(), None)
    );
}
