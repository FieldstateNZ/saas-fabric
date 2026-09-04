//! A repository that accepts a connection and never answers is bounded anyway.
//!
//! # Why this is worth a test of its own
//!
//! The platform binding holds a lock across every one of these calls, so that
//! changing which repository is bound waits for the work already running
//! against the old one. That makes the longest one of these calls can take the
//! longest an operator's disconnect can take — and a disconnect is itself cut
//! off by the API's request timeout.
//!
//! So an operation that could run unboundedly is not a slow sweep, it is a
//! disconnect that returns `504` having released nothing: the operator is told
//! it failed, and the platform is still pointed at the repository they asked it
//! to forget. Bounding each *request* does not fix it, because an operation is
//! around thirty requests. These pin the budget that bounds the operation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use fabric_git_host::GitCredential;
use fabric_platform_git::{PlatformGitRepository, PlatformRepositoryConfig};
use fabric_platform_management::{DesiredState, DesiredStateError};
use tokio::net::TcpListener;

/// A clock that does not move, because nothing here reads the time.
struct TestClock;

impl fabric_core::Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_787_907_600
    }
}

/// A host that completes the TCP handshake and then says nothing, ever.
///
/// Deliberately not a slow *responder*. The failure this guards against is the
/// one a per-request timeout cannot see: a connection that is healthy at every
/// layer below HTTP, so nothing errors and nothing retries — the call simply
/// never returns.
async fn a_host_that_never_answers() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");

    tokio::spawn(async move {
        let mut held = Vec::new();

        while let Ok((stream, _)) = listener.accept().await {
            // Kept alive rather than dropped: dropping would close the socket
            // and the client would see a clean EOF, which is an error it can
            // report quickly. Holding it is what makes the caller wait.
            held.push(stream);
        }
    });

    format!("http://{address}")
}

/// A repository pointed at that host, with a one-second operation budget and a
/// per-request timeout far too long to be what fires.
fn repository(base_url: String) -> PlatformGitRepository {
    PlatformGitRepository::new(
        &PlatformRepositoryConfig {
            api_base_url: base_url,
            owner: "FieldstateNZ".to_owned(),
            repository: "saas-fabric-platform".to_owned(),
            branch: "main".to_owned(),
            // Thirty times the operation budget, so a per-request timeout
            // cannot be what ends the call. If this test ever passes because
            // of this value, it is testing nothing.
            http_timeout_seconds: 30,
            operation_timeout_seconds: 1,
        },
        GitCredential::token("test-bearer"),
        Arc::new(TestClock),
    )
    .expect("the configuration is valid")
}

#[tokio::test]
async fn a_read_against_a_host_that_never_answers_gives_up_on_its_budget() {
    let repository = repository(a_host_that_never_answers().await);
    let started = Instant::now();

    let failure = tokio::time::timeout(Duration::from_secs(10), repository.components("lucentroot"))
        .await
        .expect("the operation must bound itself rather than be rescued by this timeout")
        .expect_err("a host that never answers cannot produce components");

    assert!(
        matches!(failure, DesiredStateError::Unavailable { .. }),
        "a repository that said nothing is unavailable, not a refusal: {failure:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it gave up after {:?}, which is not a one-second budget",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_write_against_a_host_that_never_answers_gives_up_on_its_budget() {
    // The one that matters most: a write is what holds the binding while an
    // operator is trying to disconnect, and it is the longest of these calls.
    let repository = repository(a_host_that_never_answers().await);
    let started = Instant::now();

    let failure = tokio::time::timeout(
        Duration::from_secs(10),
        repository.resume(
            "lucentroot",
            "saas-fabric",
            &fabric_platform_management::DesiredRevision::new("sha"),
            "Resume",
        ),
    )
    .await
    .expect("the operation must bound itself rather than be rescued by this timeout")
    .expect_err("a host that never answers cannot complete a write");

    assert!(
        matches!(failure, DesiredStateError::Unavailable { .. }),
        "{failure:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn the_budget_it_gave_up_on_is_what_the_operator_is_told() {
    // The detail is what reaches a log line and, through the error, a console.
    // It must name the budget and nothing else — no URL, no path, no bearer.
    let repository = repository(a_host_that_never_answers().await);

    let failure = repository
        .components("lucentroot")
        .await
        .expect_err("a host that never answers cannot produce components");

    let DesiredStateError::Unavailable { detail } = failure else {
        panic!("expected an unavailable repository");
    };

    assert!(
        detail.contains("1-second budget"),
        "the budget is named: {detail}"
    );
    assert!(!detail.contains("127.0.0.1"), "no address: {detail}");
    assert!(!detail.contains("test-bearer"), "no credential: {detail}");
}
