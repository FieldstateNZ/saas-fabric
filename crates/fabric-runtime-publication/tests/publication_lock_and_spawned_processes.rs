//! The publication lock against processes spawned from other threads, as a
//! test binary that drives `docker` does: a second publication that starts
//! after the first has returned is never refused as "in progress".

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use fabric_runtime_publication::RuntimePublication as _;
use support::TempDir;

/// Before the lock unlocked explicitly, about three in four of these
/// second publications were refused while the spawning threads ran.
#[tokio::test]
async fn sequential_publications_are_not_refused_while_other_threads_spawn_processes() {
    let stop = Arc::new(AtomicBool::new(false));
    let spawners: Vec<_> = (0..4)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::process::Command::new("true").status();
                }
            })
        })
        .collect();

    let mut refused = Vec::new();
    for round in 0..100 {
        let dir = TempDir::new();
        dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
        if let Err(error) = dir.publisher().publish(&support::base_snapshot(2)).await {
            refused.push(format!("round {round}: {error}"));
        }
    }

    stop.store(true, Ordering::Relaxed);
    for spawner in spawners {
        spawner.join().unwrap();
    }
    assert_eq!(refused, Vec::<String>::new());
}
