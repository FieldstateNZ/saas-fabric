//! The test store keeps the port's contract, so the router tests that rely
//! on it prove what they claim: one frozen operation at a time, replayed only
//! exactly, completed or released only by its own token.

use fabric_client_model::lifecycle::{IdentityInstance, Tombstone};
use fabric_client_model::{ClientId, ClientRevision, LifecycleRecord, RealmName};

use super::InMemoryLifecycleContinuity;
use crate::{ContinuityRefusal, LifecycleContinuity, LifecycleFingerprint, Transition, TransitionKind};

fn acme() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

fn instance() -> IdentityInstance {
    IdentityInstance::try_new(&"a".repeat(32)).unwrap()
}

fn fingerprint(record: &LifecycleRecord) -> LifecycleFingerprint {
    LifecycleFingerprint::of(&acme(), &RealmName::try_new("acme").unwrap(), record)
}

fn tombstone() -> Tombstone {
    Tombstone {
        at: 1,
        by: "o".to_owned(),
        reason: "r".to_owned(),
    }
}

/// A store holding `acme` tombstoned, and the record a restore moves it to.
fn tombstoned() -> (InMemoryLifecycleContinuity, LifecycleRecord, LifecycleRecord) {
    let from = LifecycleRecord {
        instance: Some(instance()),
        ..LifecycleRecord::default()
    };
    let from = from.tombstone(1, "o", "r").unwrap();
    let to = from.restore(2, "o", "renewed").unwrap();
    let store = InMemoryLifecycleContinuity::new();
    store.enroll(&acme(), instance(), fingerprint(&from));
    (store, from, to)
}

/// What every restore transition in these tests shares.
struct Parts {
    client: ClientId,
    instance: IdentityInstance,
    at: ClientRevision,
    from: LifecycleFingerprint,
    reversing: Tombstone,
}

impl Parts {
    fn new(from: &LifecycleRecord) -> Self {
        Self {
            client: acme(),
            instance: instance(),
            at: ClientRevision::try_new("rev-1").unwrap(),
            from: fingerprint(from),
            reversing: tombstone(),
        }
    }

    fn restoring<'a>(
        &'a self,
        to: &'a LifecycleFingerprint,
        record: &'a LifecycleRecord,
        operator: &'a str,
    ) -> Transition<'a> {
        Transition {
            client: &self.client,
            kind: TransitionKind::Restore,
            instance: &self.instance,
            expected_sequence: Some(0),
            at_revision: Some(&self.at),
            from: Some(&self.from),
            to,
            to_record: record,
            reversing: Some(&self.reversing),
            operator,
        }
    }
}

#[tokio::test]
async fn a_pending_transition_is_frozen_and_only_its_exact_replay_is_handed_back() {
    let (store, from, to) = tombstoned();
    let parts = Parts::new(&from);
    let to_print = fingerprint(&to);

    let first = store
        .reserve(&parts.restoring(&to_print, &to, "o"))
        .await
        .unwrap();
    let replayed = store
        .reserve(&parts.restoring(&to_print, &to, "o"))
        .await
        .unwrap();
    assert_eq!(first, replayed);

    let changed = from.restore(2, "o", "another reason").unwrap();
    let changed_print = fingerprint(&changed);
    for (record, print, operator) in [(&changed, &changed_print, "o"), (&to, &to_print, "someone-else")] {
        assert_eq!(
            store.reserve(&parts.restoring(print, record, operator)).await,
            Err(ContinuityRefusal::Pending)
        );
    }
    let pending = store.accepted(&acme()).await.unwrap().unwrap().pending.unwrap();
    assert_eq!(pending.to_record, to, "the destination never moved");
}

#[tokio::test]
async fn a_token_completes_or_releases_only_its_own_operation() {
    let (store, from, to) = tombstoned();
    let parts = Parts::new(&from);
    let to_print = fingerprint(&to);
    let first = store
        .reserve(&parts.restoring(&to_print, &to, "o"))
        .await
        .unwrap();
    store.release(&first).await;

    let second = store
        .reserve(&parts.restoring(&to_print, &to, "o"))
        .await
        .unwrap();
    assert_ne!(first, second, "a new operation never reuses a token");

    store.release(&first).await;
    assert_eq!(
        store
            .complete(&first, &ClientRevision::try_new("rev-2").unwrap())
            .await,
        Err(ContinuityRefusal::StaleCompletion)
    );
    assert!(store.accepted(&acme()).await.unwrap().unwrap().pending.is_some());

    store
        .complete(&second, &ClientRevision::try_new("rev-2").unwrap())
        .await
        .unwrap();
    let accepted = store.accepted(&acme()).await.unwrap().unwrap();
    assert_eq!((accepted.sequence, accepted.fingerprint), (1, to_print));
}
