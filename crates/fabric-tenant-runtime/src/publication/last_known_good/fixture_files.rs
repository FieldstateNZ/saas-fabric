//! Broken generations, and putting fixture generations on disk.

use super::fixtures::{published, recorded, Published};
use crate::publication::digest::sha256_hex;
use crate::publication::READER_CONTRACT;
use crate::testing::tenant_binding;

/// The ways a generation can match every digest and still be unservable,
/// each applied to generation `generation` of the fixture: the document it
/// breaks, and the generation with it broken.
pub(super) fn unservable(generation: u64) -> Vec<(&'static str, Published)> {
    let valid = || published(generation, "pg-1");
    let unbound = serde_json::to_vec(&vec![tenant_binding("acme", generation, "pg-missing")]).unwrap();
    let mut empty_binding: serde_json::Value = serde_json::from_slice(&valid().tenants).unwrap();
    empty_binding[0]["data"] = serde_json::json!({});
    let breaks = [
        (
            "tenants",
            "malformed tenants",
            Published {
                tenants: b"[{ torn".to_vec(),
                ..valid()
            },
        ),
        (
            "tenants",
            "a binding to an absent data source",
            Published {
                tenants: unbound,
                ..valid()
            },
        ),
        (
            "tenants",
            "a tenant with no data binding",
            Published {
                tenants: serde_json::to_vec(&empty_binding).unwrap(),
                ..valid()
            },
        ),
        (
            "data-sources",
            "malformed data sources",
            Published {
                data_sources: b"{}".to_vec(),
                ..valid()
            },
        ),
        (
            "catalog",
            "a catalogue the host cannot decode",
            Published {
                catalog: b"[]".to_vec(),
                ..valid()
            },
        ),
    ];
    breaks
        .into_iter()
        .map(|(document, _, files)| (document, recorded(generation, READER_CONTRACT, files)))
        .collect()
}

/// Writes `files` where a publisher would, in `directory`.
pub(super) fn publish_into(directory: &std::path::Path, files: &Published) {
    std::fs::write(directory.join("data-sources.json"), &files.data_sources).unwrap();
    std::fs::write(directory.join("catalog.json"), &files.catalog).unwrap();
    std::fs::write(directory.join("tenants.json"), &files.tenants).unwrap();
    std::fs::write(directory.join("publication.json"), &files.record).unwrap();
}

/// Writes `files` as a last-known-good copy at `path`, bypassing every
/// check the reader makes before it persists, as a copy written by an
/// older reader or edited by hand would be.
pub(super) fn write_copy(path: &std::path::Path, files: &Published) {
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).unwrap();
    let envelope = super::envelope::Envelope {
        format: super::envelope::FORMAT,
        record_sha256: sha256_hex(&files.record),
        record: text(&files.record),
        data_sources: text(&files.data_sources),
        tenants: text(&files.tenants),
        catalog: text(&files.catalog),
    };
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
}
