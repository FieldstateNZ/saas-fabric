//! Reads asked together: every one in flight at once, answered in the order
//! given, and all abandoned when the evaluation is.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::fake_registry_tests::{image_digest, pins, FakeRegistry, CONSOLE, CONTROL_PLANE, RUNTIME};
use super::together::{together, Read};
use super::{evaluate, Evaluation, Expectation, InvalidReason};
use crate::{Attached, Registry, RegistryError, Resolved};

const VERSION: &str = "0.3.0-preview.3";
const COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A registry that counts how many reads are in flight at once.
#[derive(Default)]
struct Counting {
    inner: FakeRegistry,
    in_flight: AtomicUsize,
    most: AtomicUsize,
}

#[async_trait::async_trait]
impl Registry for Counting {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        self.inner.tags(repository).await
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        // A read that takes a while: every other read gets its turn first.
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        let answer = self.inner.resolve(repository, reference).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        answer
    }

    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        self.inner.component_descriptor(repository, subject).await
    }
}

#[tokio::test]
async fn the_images_other_than_the_primary_are_asked_at_once() {
    let registry = Counting::default();
    registry.inner.publish(VERSION, COMMIT);
    let pinned = pins();

    let answer = evaluate(
        &registry,
        RUNTIME,
        VERSION,
        Expectation::Pinned {
            primary: "runtime",
            repositories: &pinned,
        },
    )
    .await
    .expect("the fake registry answers");

    assert!(matches!(answer, Evaluation::Complete(_)), "{answer:?}");
    // Two images besides the primary, each asked by digest and by tag: all
    // four were in flight together, where one after another would be one.
    assert_eq!(registry.most.load(Ordering::SeqCst), 4);
}

/// Answers `value` after yielding `turns` times.
async fn after(turns: usize, value: usize) -> usize {
    for _ in 0..turns {
        tokio::task::yield_now().await;
    }
    value
}

#[tokio::test]
async fn answers_come_back_in_the_order_the_reads_were_given() {
    // The last read answers first; the answers still line up with the reads.
    let reads: Vec<Read<'_, usize>> = vec![
        Box::pin(after(6, 0)),
        Box::pin(after(3, 1)),
        Box::pin(after(0, 2)),
    ];

    assert_eq!(together(reads, |_| false).await, vec![Some(0), Some(1), Some(2)]);
}

/// Counts itself dropped.
struct Dropped(Arc<AtomicUsize>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn dropping_the_reads_abandons_every_one_still_in_flight() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let never = |dropped: &Arc<AtomicUsize>| -> Read<'static, ()> {
        let guard = Dropped(Arc::clone(dropped));
        Box::pin(async move {
            let _held = guard;
            std::future::pending::<()>().await;
        })
    };
    let reads = vec![never(&dropped), never(&dropped), never(&dropped)];

    // Polled once, so every read has started, then given up on.
    tokio::select! {
        biased;
        _ = together(reads, |_| false) => panic!("a read that never answers answered"),
        () = std::future::ready(()) => {}
    }

    assert_eq!(dropped.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn reads_still_out_are_dropped_once_the_answers_settle() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let guard = Dropped(Arc::clone(&dropped));
    let reads: Vec<Read<'_, usize>> = vec![
        Box::pin(after(0, 7)),
        Box::pin(async move {
            let _held = guard;
            std::future::pending::<usize>().await
        }),
    ];

    let answers = together(reads, |so_far| so_far.first().is_some_and(Option::is_some)).await;

    assert_eq!(answers, vec![Some(7), None]);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

/// A registry whose reads of one reference in one repository never answer.
struct Hanging {
    inner: FakeRegistry,
    repository: &'static str,
    reference: String,
}

#[async_trait::async_trait]
impl Registry for Hanging {
    async fn tags(&self, repository: &str) -> Result<Vec<String>, RegistryError> {
        self.inner.tags(repository).await
    }

    async fn resolve(&self, repository: &str, reference: &str) -> Result<Option<Resolved>, RegistryError> {
        if repository == self.repository && reference == self.reference {
            std::future::pending::<()>().await;
        }
        self.inner.resolve(repository, reference).await
    }

    async fn component_descriptor(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        self.inner.component_descriptor(repository, subject).await
    }
}

/// Evaluates against `registry`, or `None` if it has not answered after
/// many turns of every read that can answer.
async fn within_turns(registry: &Hanging) -> Option<Result<Evaluation, RegistryError>> {
    let pinned = pins();
    let asking = evaluate(
        registry,
        RUNTIME,
        VERSION,
        Expectation::Pinned {
            primary: "runtime",
            repositories: &pinned,
        },
    );
    tokio::select! {
        biased;
        answer = asking => Some(answer),
        _ = after(64, 0) => None,
    }
}

#[tokio::test]
async fn a_missing_image_is_answered_while_a_later_read_hangs() {
    // The console's image is gone; the control plane's tag never answers. One
    // read at a time, the rule stopped at the console: so does this.
    let registry = Hanging {
        inner: FakeRegistry::default(),
        repository: CONTROL_PLANE,
        reference: VERSION.to_owned(),
    };
    registry.inner.publish(VERSION, COMMIT);
    registry.inner.delete(CONSOLE, &image_digest(VERSION, "console"));

    let answer = within_turns(&registry).await.expect("the answer is settled");

    assert_eq!(
        answer,
        Ok(Evaluation::Invalid(InvalidReason::MissingImage {
            role: "console".to_owned()
        }))
    );
}

#[tokio::test]
async fn a_registry_error_is_answered_while_a_later_read_hangs() {
    let registry = Hanging {
        inner: FakeRegistry::default(),
        repository: CONTROL_PLANE,
        reference: VERSION.to_owned(),
    };
    registry.inner.publish(VERSION, COMMIT);
    registry.inner.fail_on(&image_digest(VERSION, "console"));

    let answer = within_turns(&registry).await.expect("the answer is settled");

    assert!(
        matches!(answer, Err(RegistryError::Unavailable { .. })),
        "{answer:?}"
    );
}

#[tokio::test]
async fn a_later_failure_waits_for_a_read_the_rule_looks_at_first() {
    // The control plane's image read hangs; the console's tag is gone. One
    // read at a time, the rule would still be waiting on the control plane,
    // whose answer could be *missing* -- so nothing is answered yet.
    let registry = Hanging {
        inner: FakeRegistry::default(),
        repository: CONTROL_PLANE,
        reference: image_digest(VERSION, "controlPlane"),
    };
    registry.inner.publish(VERSION, COMMIT);
    registry.inner.untag(CONSOLE, VERSION);

    assert_eq!(within_turns(&registry).await, None);
}
