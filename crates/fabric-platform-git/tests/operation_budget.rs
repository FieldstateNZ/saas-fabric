//! An operation is bounded, and bounded without ever dropping a call it made.
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
//! around thirty requests.
//!
//! But the obvious bound — a timeout around the operation — is its own version
//! of the same bug. It drops the future, and a future dropped mid-write
//! releases the binding with the write's last request possibly already on the
//! wire, so the disconnect returns and the write lands afterwards. The budget
//! is therefore a gate on *starting* a request and never a cancellation of one
//! already sent. These pin both halves: nothing started is abandoned, and
//! nothing new starts once the budget is spent.

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

mod support;

use support::{FakePlatformHost, RecordedRequest, BRANCH, OWNER, REPOSITORY};

/// Where the environment this test drives keeps its manifest.
const MANIFEST: &str = "environments/lucentroot/components.yaml";

/// A manifest as the platform repository writes it, with one component.
///
/// `resume` touches this file and nothing else — a hold moves no version, so no
/// overlay is read — which is why the host below serves one file.
const MANIFEST_TEXT: &str = r"# What LucentRoot is asked to run, and the policy that moves it.
#
# Machine-managed. Editing it by hand is the break-glass path.
---
schemaVersion: 2
environment: lucentroot
managedRoots:
  - applications/
components:
  saas-fabric:
    artifact:
      type: oci
      sourceRevision: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
      images:
        runtime:
          repository: ghcr.io/fieldstatenz/saas-fabric
          digest: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    channel: preview
    update: automatic
    desired:
      version: 0.3.0-preview.1
    pinnedIn:
      - renderer: kustomize-image
        path: applications/core/saas-fabric/overlays/lucentroot/kustomization.yaml
        image: runtime
    hold: null
";

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

/// A repository with a one-second operation budget, over the given host.
///
/// `http_timeout_seconds` is the caller's, because it is now half the bound: an
/// operation runs for the budget plus however long the one request it may still
/// have started is allowed to take.
fn repository(base_url: String, http_timeout_seconds: u64) -> PlatformGitRepository {
    PlatformGitRepository::new(
        &PlatformRepositoryConfig {
            api_base_url: base_url,
            owner: OWNER.to_owned(),
            repository: REPOSITORY.to_owned(),
            branch: BRANCH.to_owned(),
            http_timeout_seconds,
            operation_timeout_seconds: 1,
        },
        GitCredential::token("test-bearer"),
        Arc::new(TestClock),
    )
    .expect("the configuration is valid")
}

/// A host serving the manifest, sitting on whichever calls `slow` picks out.
async fn a_host_that_answers_slowly(slow: fn(&RecordedRequest) -> bool, by: Duration) -> FakePlatformHost {
    FakePlatformHost::start_delaying(
        &[(MANIFEST, MANIFEST_TEXT)],
        Arc::new(move |request| if slow(request) { by } else { Duration::ZERO }),
    )
    .await
}

/// The call that moves the branch — the last one a write makes, and the only
/// one whose abandonment could leave a commit nobody is expecting.
fn is_the_ref_update(request: &RecordedRequest) -> bool {
    request.method == "PATCH" && request.path.contains("/git/refs/heads/")
}

/// The first call any operation makes.
fn is_the_branch_head(request: &RecordedRequest) -> bool {
    request.method == "GET" && request.path.contains("/git/ref/heads/")
}

#[tokio::test]
async fn a_request_already_in_flight_is_never_abandoned_when_the_budget_expires() {
    // The finding this closes. The ref update is sent well inside the budget
    // and answered well outside it, but inside the per-request timeout. A
    // budget that cancelled would drop the write here — releasing the binding
    // while the host was still applying the commit — and the operator would be
    // told the platform had stopped writing to a repository it then wrote to.
    let host = a_host_that_answers_slowly(is_the_ref_update, Duration::from_millis(2500)).await;
    let repository = repository(host.base_url.clone(), 5);

    let at = repository
        .component("lucentroot", "saas-fabric")
        .await
        .expect("the manifest reads")
        .revision;

    let started = Instant::now();

    repository
        .resume("lucentroot", "saas-fabric", &at, "Resume")
        .await
        .expect("a request the operation had already sent must run to its outcome");

    let elapsed = started.elapsed();

    assert!(
        host.completed().iter().any(|request| request.contains("PATCH")),
        "the host answered the ref update in full: {:?}",
        host.completed()
    );
    assert!(
        elapsed >= Duration::from_secs(2),
        "the operation waited out a call that outlasted its budget, not {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "and it is still bounded by the budget plus that one request, not {elapsed:?}"
    );
}

#[tokio::test]
async fn no_request_starts_once_the_budget_is_spent() {
    // The other half. Not abandoning what is in flight must not become "runs
    // for as long as the host likes": the request after the deadline is refused
    // rather than sent, so the operation ends one request past its budget.
    let host = a_host_that_answers_slowly(is_the_branch_head, Duration::from_secs(2)).await;
    let repository = repository(host.base_url.clone(), 5);

    let started = Instant::now();
    let deadline = started + Duration::from_secs(1);

    let failure = repository
        .components("lucentroot")
        .await
        .expect_err("the budget was spent reading the branch head");

    let DesiredStateError::Unavailable { detail } = failure else {
        panic!("expected an unavailable repository");
    };
    assert!(
        detail.contains("1-second budget"),
        "the budget is named: {detail}"
    );

    assert!(
        host.request_times().iter().all(|at| *at <= deadline),
        "every request began inside the budget; {} of them were made",
        host.request_times().len()
    );
    assert_eq!(
        host.paths().len(),
        1,
        "the manifest read was never started: {:?}",
        host.paths()
    );
}

#[tokio::test]
async fn a_read_against_a_host_that_never_answers_gives_up_on_its_budget() {
    let repository = repository(a_host_that_never_answers().await, 2);
    let started = Instant::now();

    let failure = tokio::time::timeout(Duration::from_secs(10), repository.components("lucentroot"))
        .await
        .expect("the operation must bound itself rather than be rescued by this timeout")
        .expect_err("a host that never answers cannot produce components");

    assert!(
        matches!(failure, DesiredStateError::Unavailable { .. }),
        "a repository that said nothing is unavailable, not a refusal: {failure:?}"
    );
    // The bound is the budget plus one request timeout, because the request
    // that was already sent is waited out rather than dropped.
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it gave up after {:?}, which is not a one-second budget plus a two-second request",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_write_against_a_host_that_never_answers_gives_up_on_its_budget() {
    // The one that matters most: a write is what holds the binding while an
    // operator is trying to disconnect, and it is the longest of these calls.
    let repository = repository(a_host_that_never_answers().await, 2);
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
    //
    // A host that answers slowly rather than one that never answers, because
    // the two failures are different and this is about the budget's own. A host
    // that says nothing at all is a request that times out, and the operator is
    // told that instead.
    let host = a_host_that_answers_slowly(is_the_branch_head, Duration::from_secs(2)).await;
    let repository = repository(host.base_url.clone(), 5);

    let failure = repository
        .components("lucentroot")
        .await
        .expect_err("the budget was spent before the manifest could be read");

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
