//! What the reader-compatibility suites share: resolving a tenant, waiting
//! for a refresh, and publishing bytes no publisher in this workspace writes
//! under a record that names them exactly.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use fabric_core::{LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    DocumentRevision, PublicationRecord, RecordedDocument, RecordedDocuments, ResourceHistory,
    PUBLICATION_RECORD_FILE,
};
use fabric_tenant_runtime::{ResolveError, RuntimeResolver};

use super::{poll_for_load_count_above, Stack, TempDir};

/// The id of the DataSource `tenant`'s `primary` data resolves to.
///
/// # Errors
///
/// The resolver's refusal.
pub fn resolves_to(resolver: &RuntimeResolver, tenant: &str) -> Result<String, ResolveError> {
    resolver
        .resolve_data_source(
            &TenantId::try_new(tenant).unwrap(),
            &LogicalDataSourceName::try_new("primary").unwrap(),
        )
        .map(|resolved| resolved.data_source.id.to_string())
}

/// Polls `condition` until it holds, or fails the test after two seconds.
pub async fn poll_until(what: &str, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < Duration::from_secs(2), "{what} did not happen");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Asks the stack's reader to refresh, and waits until a pass has read.
pub async fn refresh(stack: &Stack) {
    let before = stack.loads.load(Ordering::SeqCst);
    stack.handles.refresh_now();
    poll_for_load_count_above(&stack.loads, before, Duration::from_secs(2)).await;
}

/// Writes a record over the files in `dir` that names them exactly, with
/// `edit` applied to the record's JSON -- how a test publishes a generation
/// under a contract this publisher does not write.
pub fn rewrite_record(dir: &TempDir, generation: u64, edit: impl FnOnce(&mut serde_json::Value)) {
    let documents = RecordedDocuments {
        tenants: RecordedDocument::of(DocumentRevision::new(generation), &dir.read_raw("tenants.json")),
        data_sources: RecordedDocument::of(
            DocumentRevision::new(generation),
            &dir.read_raw("data-sources.json"),
        ),
        catalog: RecordedDocument::of(DocumentRevision::new(1), &dir.read_raw("catalog.json")),
    };
    let record = PublicationRecord::new(generation, documents, ResourceHistory::default());
    let mut json: serde_json::Value = serde_json::from_slice(&record.canonical_json().unwrap()).unwrap();
    edit(&mut json);
    dir.write_raw(
        PUBLICATION_RECORD_FILE,
        &serde_json::to_vec_pretty(&json).unwrap(),
    );
}

/// Rewrites the JSON file `name` in `dir` through `edit`, bypassing the
/// publisher: how a test stands in for a publisher whose document shape
/// differs from this one's.
pub fn edit_json(dir: &TempDir, name: &str, edit: impl FnOnce(&mut serde_json::Value)) {
    let mut json: serde_json::Value = serde_json::from_slice(&dir.read_raw(name)).unwrap();
    edit(&mut json);
    dir.write_raw(name, &serde_json::to_vec_pretty(&json).unwrap());
}
