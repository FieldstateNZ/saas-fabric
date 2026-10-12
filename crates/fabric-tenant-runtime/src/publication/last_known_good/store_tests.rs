//! The copy on disk: persisted whole, refused when it does not check out,
//! inspected read-only, and discarded only with an audit record.

use std::io;

use super::atomic_replace::Step;
use super::envelope::Envelope;
use super::fixtures::{complete, published_with_floor};
use super::{DiscardRequest, LastKnownGoodRefusal, LastKnownGoodStore, Verdict, AUDIT_FILE_NAME};
use crate::publication::digest::sha256_hex;
use crate::publication::READER_CONTRACT;
use crate::testing::TempDir;

fn store(dir: &TempDir) -> LastKnownGoodStore {
    LastKnownGoodStore::new(dir.path().join("lkg"))
}

async fn persisted(dir: &TempDir, generation: u64) -> LastKnownGoodStore {
    let store = store(dir);
    store
        .persist(&complete(generation, "pg-1"), |_| Ok(()))
        .await
        .unwrap();
    store
}

/// Rewrites the copy's envelope through `edit`, as bit rot or a hand edit
/// would, without touching anything else.
fn tamper(store: &LastKnownGoodStore, edit: impl FnOnce(&mut Envelope)) {
    let mut envelope: Envelope = serde_json::from_slice(&std::fs::read(store.path()).unwrap()).unwrap();
    edit(&mut envelope);
    std::fs::write(store.path(), serde_json::to_vec(&envelope).unwrap()).unwrap();
}

fn discard_request() -> DiscardRequest {
    DiscardRequest {
        operator: "ops@example.test".to_owned(),
        reason: "incident 42: copy held a revoked binding".to_owned(),
    }
}

#[tokio::test]
async fn a_persisted_generation_reads_back_byte_for_byte_and_validates() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 3).await;

    let loaded = store.load().await.unwrap().unwrap();
    let original = complete(3, "pg-1");

    assert_eq!(loaded.generation, 3);
    assert_eq!(loaded.record, original.record);
    assert_eq!(loaded.tenants, original.tenants);
    assert_eq!(loaded.data_sources, original.data_sources);
    assert_eq!(loaded.catalog, original.catalog);
}

#[tokio::test]
async fn no_copy_is_none_rather_than_a_refusal() {
    let dir = TempDir::new("lkg-store");
    assert!(store(&dir).load().await.unwrap().is_none());
}

#[tokio::test]
async fn a_copy_whose_document_no_longer_matches_its_digest_is_refused() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 3).await;
    tamper(&store, |envelope| {
        envelope.tenants = envelope.tenants.replace("pg-1", "pg-9");
    });

    let refused = store.load().await.unwrap_err();

    assert!(
        matches!(
            refused,
            LastKnownGoodRefusal::DigestMismatch { document: "tenants" }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_copy_whose_record_was_altered_is_refused_by_the_records_own_digest() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 3).await;
    tamper(&store, |envelope| {
        envelope.record = envelope.record.replace("\"generation\": 3", "\"generation\": 30");
    });

    let refused = store.load().await.unwrap_err();

    assert!(
        matches!(
            refused,
            LastKnownGoodRefusal::DigestMismatch { document: "record" }
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_copy_whose_record_requires_a_newer_reader_is_refused() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 3).await;
    let ahead = published_with_floor(3, "pg-1", READER_CONTRACT + 1).record;
    tamper(&store, |envelope| {
        envelope.record = String::from_utf8(ahead).unwrap();
        envelope.record_sha256 = sha256_hex(envelope.record.as_bytes());
    });

    let refused = store.load().await.unwrap_err();

    assert!(
        matches!(
            refused,
            LastKnownGoodRefusal::UnsupportedContract { required, supported }
                if required == READER_CONTRACT + 1 && supported == READER_CONTRACT
        ),
        "{refused}"
    );
}

#[tokio::test]
async fn a_copy_in_a_format_this_reader_does_not_know_is_refused() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 3).await;
    tamper(&store, |envelope| envelope.format = 2);

    let refused = store.load().await.unwrap_err();

    assert!(
        matches!(refused, LastKnownGoodRefusal::UnsupportedFormat { found: 2 }),
        "{refused}"
    );
}

#[tokio::test]
async fn a_truncated_copy_is_refused_as_malformed() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 3).await;
    let bytes = std::fs::read(store.path()).unwrap();
    std::fs::write(store.path(), &bytes[..bytes.len() / 2]).unwrap();

    let refused = store.load().await.unwrap_err();

    assert!(
        matches!(refused, LastKnownGoodRefusal::Malformed { .. }),
        "{refused}"
    );
}

#[tokio::test]
async fn a_generation_read_without_its_catalogue_is_not_persisted() {
    let dir = TempDir::new("lkg-store");
    let store = store(&dir);
    let without = complete(3, "pg-1").attach_catalogue_if_matching(None);

    assert!(store.persist(&without, |_| Ok(())).await.is_err());
    assert!(!store.path().exists());
}

#[tokio::test]
async fn an_interrupted_persist_leaves_the_previous_copy_valid() {
    for step in [Step::Written, Step::Synced] {
        let dir = TempDir::new("lkg-store");
        let store = persisted(&dir, 3).await;

        let interrupted = store
            .persist(&complete(4, "pg-2"), |at| {
                if at == step {
                    Err(io::Error::other("power cut"))
                } else {
                    Ok(())
                }
            })
            .await;

        assert!(interrupted.is_err(), "{step:?}");
        let loaded = store.load().await.unwrap().unwrap();
        assert_eq!(loaded.generation, 3, "{step:?}");
    }
}

#[tokio::test]
async fn inspecting_reports_absent_valid_and_refused_copies_and_writes_nothing() {
    let dir = TempDir::new("lkg-store");
    let store = store(&dir);
    assert_eq!(store.inspect().await.verdict, Verdict::Absent);

    store.persist(&complete(5, "pg-1"), |_| Ok(())).await.unwrap();
    let valid = store.inspect().await;
    assert!(
        matches!(valid.verdict, Verdict::Valid { generation: 5, .. }),
        "{valid:?}"
    );
    assert_eq!(valid.stated_generation, Some(5));

    tamper(&store, |envelope| envelope.catalog.push(' '));
    let before = std::fs::read(store.path()).unwrap();
    let refused = store.inspect().await;
    assert!(matches!(refused.verdict, Verdict::Refused { .. }), "{refused:?}");
    assert_eq!(
        refused.stated_generation,
        Some(5),
        "a refused copy still says what it claims"
    );
    assert_eq!(std::fs::read(store.path()).unwrap(), before);
}

#[tokio::test]
async fn discarding_audits_first_then_moves_the_copy_aside_and_writes_nothing_in_its_place() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 6).await;
    let before = std::fs::read(store.path()).unwrap();

    let discarded = store
        .discard(&discard_request(), 1_700_000_000)
        .await
        .unwrap()
        .unwrap();

    assert!(!store.path().exists(), "nothing is written in the copy's place");
    assert!(store.load().await.unwrap().is_none());
    assert_eq!(std::fs::read(&discarded.moved_to).unwrap(), before);
    assert!(matches!(
        discarded.inspection.verdict,
        Verdict::Valid { generation: 6, .. }
    ));

    let audit = std::fs::read_to_string(store.path().with_file_name(AUDIT_FILE_NAME)).unwrap();
    let lines: Vec<serde_json::Value> = audit
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["action"], "discard");
    assert_eq!(line["operator"], "ops@example.test");
    assert_eq!(line["reason"], "incident 42: copy held a revoked binding");
    assert_eq!(line["at_unix_seconds"], 1_700_000_000);
    assert_eq!(line["inspection"]["stated_generation"], 6);
    assert_eq!(line["moved_to"], discarded.moved_to);
}

#[tokio::test]
async fn a_refused_copy_can_be_discarded_too() {
    let dir = TempDir::new("lkg-store");
    let store = persisted(&dir, 6).await;
    tamper(&store, |envelope| envelope.tenants.push(' '));

    let discarded = store.discard(&discard_request(), 1).await.unwrap().unwrap();

    assert!(matches!(discarded.inspection.verdict, Verdict::Refused { .. }));
    assert!(!store.path().exists());
}

#[tokio::test]
async fn discarding_nothing_changes_nothing_and_audits_nothing() {
    let dir = TempDir::new("lkg-store");
    let store = store(&dir);

    assert!(store.discard(&discard_request(), 1).await.unwrap().is_none());
    assert!(!store.path().with_file_name(AUDIT_FILE_NAME).exists());
}
