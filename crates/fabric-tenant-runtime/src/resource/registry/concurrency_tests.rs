//! Item 48: proof that a full-sync swap is atomic.
//!
//! `apply_all` builds the next snapshot off to one side and installs it with
//! a single atomic pointer store (see `snapshot.rs` for why). This hammers
//! that claim under real concurrency: many readers run alongside a writer
//! that alternates between two whole generations, and every observation must
//! be consistent with exactly one of them — never a value that could only
//! exist if the swap were torn.
//!
//! Two independent, non-flaky invariants are checked, because a single
//! `lookup()` cannot by itself prove anything about the *whole* snapshot (it
//! only ever touches one key):
//!
//! - **Per-entry:** a key from generation A must never be seen at an even
//!   (generation B) revision, and vice versa. This holds even though reads
//!   are not synchronised with the writer, because it is checked from a
//!   *single* atomic load — there is no window in which it could pass on a
//!   correct implementation and no window in which timing alone could cause
//!   a false failure.
//! - **Whole-registry:** `len()` must never report a size other than the
//!   unprimed 0 or one of the two generations' exact sizes. An
//!   implementation that mutated a shared map key-by-key instead of
//!   swapping a whole snapshot would pass through many intermediate sizes;
//!   an atomic swap cannot.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use crate::resource::registry::test_resource::{registry, resource};
use crate::resource::ResourceRegistry;

/// Generation A: fifty keys, always applied at an odd revision.
const SET_A_SIZE: usize = 50;

/// Generation B: five keys, disjoint from A, always applied at an even
/// revision. The size gap makes a torn read easy to distinguish from either
/// whole snapshot.
const SET_B_SIZE: usize = 5;

/// How many full syncs the writer performs.
const PASSES: u64 = 400;

/// How many lookups each reader performs. Comfortably larger than `PASSES`
/// so readers are still running for the writer's whole lifetime.
const READS_PER_READER: u64 = 5_000;

/// How many reader tasks hammer the registry concurrently.
const READER_COUNT: usize = 8;

fn generation_a(revision: u64) -> Vec<crate::resource::registry::test_resource::TestResource> {
    (0..SET_A_SIZE)
        .map(|i| resource(&format!("a-{i}"), revision))
        .collect()
}

fn generation_b(revision: u64) -> Vec<crate::resource::registry::test_resource::TestResource> {
    (0..SET_B_SIZE)
        .map(|i| resource(&format!("b-{i}"), revision))
        .collect()
}

async fn run_writer(registry: Arc<ResourceRegistry<crate::resource::registry::test_resource::TestResource>>) {
    for pass in 1..=PASSES {
        if pass % 2 == 1 {
            registry.apply_all(generation_a(pass));
        } else {
            registry.apply_all(generation_b(pass));
        }

        if pass % 20 == 0 {
            tokio::task::yield_now().await;
        }
    }
}

async fn hammer_lookups(
    registry: Arc<ResourceRegistry<crate::resource::registry::test_resource::TestResource>>,
    observed_sizes: Arc<Mutex<HashSet<usize>>>,
) {
    for i in 0..READS_PER_READER {
        observed_sizes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(registry.len());

        let a_key = format!("a-{}", i % SET_A_SIZE as u64);
        if let Ok(value) = registry.lookup(&a_key) {
            assert_eq!(
                value.revision.get() % 2,
                1,
                "an `a-*` key must only ever be observed at an odd (generation A) revision — \
                 seeing it at an even one means a reader read a mix of two generations"
            );
        }

        let b_key = format!("b-{}", i % SET_B_SIZE as u64);
        if let Ok(value) = registry.lookup(&b_key) {
            assert_eq!(
                value.revision.get() % 2,
                0,
                "a `b-*` key must only ever be observed at an even (generation B) revision — \
                 seeing it at an odd one means a reader read a mix of two generations"
            );
        }

        if i % 50 == 0 {
            tokio::task::yield_now().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_readers_never_observe_a_torn_snapshot() {
    let registry = Arc::new(registry());
    let observed_sizes = Arc::new(Mutex::new(HashSet::new()));

    let writer = tokio::spawn(run_writer(Arc::clone(&registry)));

    let readers: Vec<_> = (0..READER_COUNT)
        .map(|_| tokio::spawn(hammer_lookups(Arc::clone(&registry), Arc::clone(&observed_sizes))))
        .collect();

    writer.await.unwrap();
    for reader in readers {
        reader.await.unwrap();
    }

    let valid_sizes: HashSet<usize> = [0, SET_A_SIZE, SET_B_SIZE].into_iter().collect();
    let sizes = observed_sizes.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        sizes.is_subset(&valid_sizes),
        "observed a registry size outside {{0, {SET_A_SIZE}, {SET_B_SIZE}}}: {sizes:?} — \
         evidence of a torn (half-applied) snapshot"
    );
}
