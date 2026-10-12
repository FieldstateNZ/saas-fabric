//! The local audit trail's directory lock outlives a dropped trail while an
//! append it started is still queued: a second owner is refused until that
//! work finishes, so two writers never extend one stream.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{SystemTime, UNIX_EPOCH};

use fabric_client_model::ClientId;
use fabric_control_plane::{AuditEntry, AuditRecord, AuditStream, AuditTrail};
use fabric_control_plane_api::local_audit_trail::{LocalAuditTrail, LocalAuditTrailError};

fn acme() -> AuditStream {
    AuditStream::Client(ClientId::try_new("acme").unwrap())
}

fn record(n: u64) -> AuditRecord {
    AuditRecord::new(acme(), "control_plane.audit.test", "test", "operator").with("revision", n)
}

#[test]
fn a_cancelled_caller_s_queued_append_holds_the_lock_until_it_finishes() {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("fabric-audit-queued-{}-{stamp}", std::process::id()));
    // One blocking thread, so an append queued behind `blocker` waits for it,
    // and blocking work runs in the order it was queued.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let trail = LocalAuditTrail::open(&dir).unwrap();
        trail.append(record(1), 1).await.unwrap();

        let (release, gate) = std::sync::mpsc::channel::<()>();
        let blocker = tokio::task::spawn_blocking(move || gate.recv().unwrap());

        // On one runtime thread the caller runs, uninterrupted, from its
        // signal to the point its append is queued and it waits.
        let (started, polled) = tokio::sync::oneshot::channel();
        let caller = tokio::spawn(async move {
            started.send(()).unwrap();
            trail.append(record(2), 2).await
        });
        polled.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());

        assert!(
            matches!(
                LocalAuditTrail::open(&dir),
                Err(LocalAuditTrailError::AlreadyOpen { .. })
            ),
            "a second owner opened while the old append was still queued"
        );

        release.send(()).unwrap();
        blocker.await.unwrap();
        tokio::task::spawn_blocking(|| ()).await.unwrap();

        let trail = LocalAuditTrail::open(&dir).unwrap();
        trail.append(record(3), 3).await.unwrap();
        let stored = trail.read(&acme()).await.unwrap();
        let sequences: Vec<u64> = stored.entries.iter().map(AuditEntry::sequence).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
        assert_eq!(stored.verify().unwrap().unwrap().sequence, 3);
    });

    let _ = std::fs::remove_dir_all(&dir);
}
