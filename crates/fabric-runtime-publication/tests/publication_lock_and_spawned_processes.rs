//! The publication lock against processes spawned from other threads, as a
//! test binary that drives `docker` does: a second publication that starts
//! after the first has returned is never refused as "in progress".

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use fabric_runtime_publication::RuntimePublication as _;
use support::TempDir;

/// Threads that spawn and wait for `true` until stopped, counting the runs
/// that succeeded. Dropping stops and joins them, so they never outlive the
/// test, even when one of its assertions fails first.
struct Spawners {
    stop: Arc<AtomicBool>,
    succeeded: Arc<AtomicUsize>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl Spawners {
    fn start(count: usize) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let succeeded = Arc::new(AtomicUsize::new(0));
        let threads = (0..count)
            .map(|_| {
                let stop = Arc::clone(&stop);
                let succeeded = Arc::clone(&succeeded);
                std::thread::spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if std::process::Command::new("true")
                            .status()
                            .is_ok_and(|status| status.success())
                        {
                            succeeded.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();
        Self {
            stop,
            succeeded,
            threads,
        }
    }

    /// Stops and joins every thread, and answers how many runs succeeded.
    fn finish(mut self) -> usize {
        self.stop_and_join();
        self.succeeded.load(Ordering::Relaxed)
    }

    fn stop_and_join(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for Spawners {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

/// Before the lock unlocked explicitly, about three in four of these
/// second publications were refused while the spawning threads ran. The
/// spawned runs must actually succeed, or the test proves nothing about
/// publications racing a spawn.
#[tokio::test]
async fn sequential_publications_are_not_refused_while_other_threads_spawn_processes() {
    let spawners = Spawners::start(4);

    let mut refused = Vec::new();
    for round in 0..100 {
        let dir = TempDir::new();
        dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
        if let Err(error) = dir.publisher().publish(&support::base_snapshot(2)).await {
            refused.push(format!("round {round}: {error}"));
        }
    }

    let succeeded = spawners.finish();
    assert!(succeeded > 0, "no spawned process ran, so nothing raced the lock");
    assert_eq!(refused, Vec::<String>::new());
}
