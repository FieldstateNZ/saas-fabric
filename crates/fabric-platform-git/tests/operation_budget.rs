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
//!
//! A gate has one blind spot, and there is a test for it too. Under the App
//! posture a bearer is minted behind a mutex every concurrent operation shares,
//! so the time an operation spends *queueing* for one falls between two checks
//! and is seen by neither. That wait is bounded rather than gated, which is the
//! one thing here a budget may cut short — a token exchange changes nothing in
//! the repository, so there is no write to abandon.

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

/// A throwaway RSA key, generated for the clients integration's tests and used
/// nowhere else.
///
/// Committed deliberately: it authenticates nothing, and generating one at
/// runtime would put a key generator in the dev-dependency graph to prove a
/// property about a queue.
const TEST_KEY: &str = include_str!("support/test-app-key.pem");

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
    budgeted(
        base_url,
        http_timeout_seconds,
        1,
        GitCredential::token("test-bearer"),
    )
}

/// The same, with the budget and the credential spelled out.
///
/// Separate because only two tests need to say either: one needs room for six
/// prompt requests before a slow one, and one needs the App posture, where a
/// bearer is minted behind a mutex rather than simply presented.
fn budgeted(
    base_url: String,
    http_timeout_seconds: u64,
    operation_timeout_seconds: u64,
    credential: GitCredential,
) -> PlatformGitRepository {
    PlatformGitRepository::new(
        &PlatformRepositoryConfig {
            api_base_url: base_url,
            owner: OWNER.to_owned(),
            repository: REPOSITORY.to_owned(),
            branch: BRANCH.to_owned(),
            http_timeout_seconds,
            operation_timeout_seconds,
        },
        credential,
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

/// The call that turns the App's key into a bearer.
///
/// Made before the first of the operation's own calls, and made behind a mutex
/// every concurrent operation queues on — which is the whole of the next test.
fn is_the_token_mint(request: &RecordedRequest) -> bool {
    request.path.contains("/access_tokens")
}

#[tokio::test]
async fn a_request_already_in_flight_is_never_abandoned_when_the_budget_expires() {
    // The finding this closes. The ref update is sent well inside the budget
    // and answered well outside it, but inside the per-request timeout. A
    // budget that cancelled would drop the write here — releasing the binding
    // while the host was still applying the commit — and the operator would be
    // told the platform had stopped writing to a repository it then wrote to.
    //
    // Six requests have to start inside the budget before the delayed one, so
    // the budget is three seconds rather than one: a runner under load can lose
    // a second to scheduling, and a test that then failed would be reporting the
    // machine rather than the property.
    let host = a_host_that_answers_slowly(is_the_ref_update, Duration::from_secs(4)).await;
    let repository = budgeted(host.base_url.clone(), 8, 3, GitCredential::token("test-bearer"));

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
        elapsed >= Duration::from_secs(4),
        "the operation waited out a call that outlasted its budget, not {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(8),
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

    let elapsed = started.elapsed();

    let DesiredStateError::Unavailable { detail } = failure else {
        panic!("expected an unavailable repository");
    };
    assert!(
        detail.contains("1-second budget"),
        "the budget is named: {detail}"
    );

    // Refusing the next request must not have become dropping this one. The
    // branch-head read outlives the budget by a second, and the operation is
    // still here when the host finishes answering it.
    assert!(
        host.completed()
            .iter()
            .any(|request| request.contains("/git/ref/heads/")),
        "the host answered the branch head in full: {:?}",
        host.completed()
    );
    assert!(
        elapsed >= Duration::from_secs(2),
        "the operation waited out the call it had already sent, not {elapsed:?}"
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
    // that was already sent is waited out rather than dropped. Both halves are
    // asserted: giving up before the request timeout would mean it *had* been
    // dropped, which is the failure the whole design exists to prevent.
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(2),
        "the request already sent ran to its own timeout, not {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "it gave up after {elapsed:?}, which is not a one-second budget plus a two-second request"
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

    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(2),
        "the request already sent ran to its own timeout, not {elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
}

/// One read, and how long it took to give up or to answer.
async fn timed(repository: &PlatformGitRepository) -> (Result<Vec<String>, DesiredStateError>, Duration) {
    let started = Instant::now();
    let outcome = repository.components("lucentroot").await;

    (outcome, started.elapsed())
}

#[tokio::test]
async fn a_stalled_token_endpoint_cannot_stretch_an_operation_past_its_bound() {
    // The bound is "the budget plus one request", and under the App posture it
    // was false. Minting holds one mutex across the token exchange, on purpose,
    // so that concurrent sweeps share a token rather than each minting their
    // own — and nothing bounded the *wait* for that mutex. With an expired token
    // and a stalled token endpoint, each operation in turn burns a whole request
    // timeout failing to mint before the next is let in. The check before the
    // bearer has already passed and the one after it has not been reached, so a
    // single `attempt` spans several request timeouts with the binding's read
    // guard held, and the drain is unbounded again by a different route.
    //
    // A third check would not close it: that bounds the mint and leaves the
    // queue in front of it, which is where the time goes.
    //
    // Two operations, one repository, one mutex. The second is the one that
    // waits: unbounded it gives up at six seconds, bounded it gives up at four.
    let host = a_host_that_answers_slowly(is_the_token_mint, Duration::from_secs(10)).await;
    let repository = budgeted(
        host.base_url.clone(),
        3,
        1,
        GitCredential::app("123456", "789", TEST_KEY),
    );

    let ((first, first_took), (second, second_took)) = tokio::join!(timed(&repository), timed(&repository));

    for outcome in [&first, &second] {
        assert!(
            matches!(outcome, Err(DesiredStateError::Unavailable { .. })),
            "a repository the platform could not authenticate to in time is unavailable, \
             not {outcome:?}"
        );
    }

    for took in [first_took, second_took] {
        assert!(
            took < Duration::from_secs(5),
            "each operation ended inside its one-second budget plus one three-second \
             request; they took {first_took:?} and {second_took:?}"
        );
    }

    // The floor is what proves the second operation genuinely queued behind
    // the first's stalled mint rather than failing fast for some other reason;
    // and the mint has to have been reached at all, or the queue this bounds
    // was never exercised.
    assert!(
        second_took >= Duration::from_secs(3),
        "the second operation waited out the first's request timeout, not {second_took:?}"
    );
    assert!(
        host.paths().iter().any(|path| path.contains("/access_tokens")),
        "the token endpoint was asked: {:?}",
        host.paths()
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
