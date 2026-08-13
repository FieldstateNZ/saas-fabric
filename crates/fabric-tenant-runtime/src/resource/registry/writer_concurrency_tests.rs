//! Proof that concurrent *writers* do not lose each other's work.
//!
//! `concurrency_tests.rs` is the reader-side proof: one writer, eight
//! readers, nobody ever sees a torn snapshot. It says nothing about two
//! writers, and with a single writer it cannot — the bug these tests exist
//! for needs two threads inside the mutators at once.
//!
//! Every mutator is a read-modify-write: load the snapshot, build the next
//! one from it, store that back. Two writers running that concurrently both
//! read the same starting snapshot, and whichever stores second overwrites
//! the other's work entirely. Two distinct failures follow, and there is one
//! test for each:
//!
//! - **A lost update.** Writers touching disjoint keys should compose — each
//!   `apply_one` adds its own key and disturbs nothing else. Under the race
//!   they clobber one another and keys simply vanish, with no error and no
//!   log line.
//! - **A revision that walks backwards.** This is the worse one, because it
//!   defeats the guard §20 exists to enforce. A writer that read revision 1
//!   and decided "my revision 3 is newer" can land *after* a writer that
//!   installed revision 5 — so the registry ends up holding 3, and a tenant
//!   is pointed back at a database a migration has already drained. The
//!   revision guard is evaluated against a snapshot that is stale by the
//!   time the store happens, which makes it no guard at all.
//!
//! Both assertions are one-sided: they can only fail on an implementation
//! that actually loses a write. Neither can fail through timing alone on a
//! correct one, because both are statements about the *final* state, taken
//! after every writer has been joined.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::resource::registry::test_resource::{registry, resource, TestResource};
use crate::resource::ResourceRegistry;

/// How many writer tasks run concurrently. Comfortably more than the worker
/// thread count, so tasks are preempted mid-mutator rather than each running
/// to completion.
const WRITER_COUNT: u64 = 8;

/// How many keys each writer inserts in the lost-update test.
const KEYS_PER_WRITER: u64 = 40;

/// How many full syncs each writer performs in the revision test.
const PASSES: u64 = 60;

/// The single key every writer in the revision test contends over.
const CONTENDED_KEY: &str = "contended";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writers_never_lose_each_others_resources() {
    let registry = Arc::new(registry());
    registry.apply_all(vec![]);

    let writers: Vec<_> = (0..WRITER_COUNT)
        .map(|writer| tokio::spawn(insert_own_keys(Arc::clone(&registry), writer)))
        .collect();

    for writer in writers {
        writer.await.unwrap();
    }

    assert_eq!(
        registry.len() as u64,
        WRITER_COUNT * KEYS_PER_WRITER,
        "every writer used its own keys, so nothing they applied could \
         legitimately displace anything else — a shortfall means one writer's \
         read-modify-write overwrote another's"
    );
}

/// One writer's share of the lost-update test: keys nobody else touches.
async fn insert_own_keys(registry: Arc<ResourceRegistry<TestResource>>, writer: u64) {
    for key in 0..KEYS_PER_WRITER {
        registry.apply_one(resource(&format!("w{writer}-k{key}"), 1));

        if key % 8 == 0 {
            tokio::task::yield_now().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_full_syncs_never_walk_the_held_revision_backwards() {
    let registry = Arc::new(registry());
    let high_water = Arc::new(AtomicU64::new(0));

    let writers: Vec<_> = (1..=WRITER_COUNT)
        .map(|writer| {
            tokio::spawn(publish_rising_revisions(
                Arc::clone(&registry),
                Arc::clone(&high_water),
                writer,
            ))
        })
        .collect();

    for writer in writers {
        writer.await.unwrap();
    }

    // No writer applies a revision above this, and the guard forbids anything
    // lower displacing it, so on a correct implementation the final held
    // revision is exactly the highest one anybody published.
    assert_eq!(
        registry.lookup(&CONTENDED_KEY.to_owned()).unwrap().revision.get(),
        PASSES * WRITER_COUNT,
        "the registry settled on a revision lower than one it had already \
         held; revisions only move forward (§20)"
    );
}

/// One writer's share of the revision test.
///
/// Each writer publishes a rising sequence of revisions and, after every
/// publish, checks what the registry now holds against the highest revision
/// *any* writer has observed. Seeing something lower means the held revision
/// decreased between the two observations, which the guard is supposed to
/// make impossible.
async fn publish_rising_revisions(
    registry: Arc<ResourceRegistry<TestResource>>,
    high_water: Arc<AtomicU64>,
    writer: u64,
) {
    for pass in 0..PASSES {
        registry.apply_all(vec![resource(CONTENDED_KEY, pass * WRITER_COUNT + writer)]);

        let observed = registry.lookup(&CONTENDED_KEY.to_owned()).unwrap().revision.get();
        let previous = high_water.fetch_max(observed, Ordering::SeqCst);

        assert!(
            observed >= previous,
            "the held revision went backwards, from {previous} to {observed} — \
             a writer evaluated the revision guard against a snapshot that was \
             already stale by the time it stored its own"
        );

        if pass % 8 == 0 {
            tokio::task::yield_now().await;
        }
    }
}
