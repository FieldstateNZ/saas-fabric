//! Not connected is a state, a connected-but-broken one is not it, and neither
//! is a repository this platform has stopped targeting.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::sync::Notify;

use super::PlatformDesiredState;
use crate::{
    ArtifactSource, Channel, ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, Release,
    ReleaseUnit, UpdatePolicy,
};

/// A repository that is reachable, and whose reads fail.
struct Connected;

#[async_trait::async_trait]
impl DesiredState for Connected {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(vec!["saas-fabric".to_owned()])
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Err(DesiredStateError::Unavailable {
            detail: "the platform repository timed out".to_owned(),
        })
    }

    async fn advance(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

fn unit() -> crate::ReleaseUnit {
    crate::ReleaseUnit {
        version: crate::Version::parse("0.3.0-preview.1").expect("a version"),
        source_revision: "abc".to_owned(),
        images: BTreeMap::new(),
    }
}

#[tokio::test]
async fn every_operation_says_not_connected_until_something_is() {
    let binding = PlatformDesiredState::unconnected();

    assert!(!binding.is_connected().await);
    assert_eq!(
        binding
            .components("lucentroot")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
    assert_eq!(
        binding
            .component("lucentroot", "saas-fabric")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
    assert_eq!(
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &DesiredRevision::new("r1"),
                "Promote",
            )
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
}

#[tokio::test]
async fn connecting_something_makes_it_answer() {
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::new(Connected)).await;

    assert!(binding.is_connected().await);
    assert_eq!(
        binding.components("lucentroot").await.unwrap(),
        vec!["saas-fabric"]
    );
}

#[tokio::test]
async fn a_connected_repository_that_fails_does_not_look_disconnected() {
    // The distinction an operator's next step depends on. "Not connected"
    // sends them to connect one; this one is already connected and broken, and
    // saying otherwise sends them to do something they have already done.
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::new(Connected)).await;

    let failure = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect_err("this repository fails that call");

    assert_ne!(failure, DesiredStateError::NotConnected);
    assert!(matches!(failure, DesiredStateError::Unavailable { .. }));
}

#[tokio::test]
async fn disconnecting_goes_back_to_not_connected() {
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::new(Connected)).await;
    binding.disconnect().await;

    assert!(!binding.is_connected().await);
    assert_eq!(
        binding.components("lucentroot").await.expect_err("disconnected"),
        DesiredStateError::NotConnected
    );
}

#[tokio::test]
async fn an_integration_that_could_not_be_built_is_failing_rather_than_absent() {
    // Somebody connected this. "Nothing is connected" would send them to
    // connect it a second time instead of to the reason the first one stopped
    // working — which is the failure this whole three-state binding exists to
    // prevent.
    let binding = PlatformDesiredState::unconnected();

    binding.unusable("the application's key could not be read").await;

    let failure = binding
        .components("lucentroot")
        .await
        .expect_err("a repository that could not be built cannot be read");

    assert_ne!(failure, DesiredStateError::NotConnected);
    assert!(
        matches!(failure, DesiredStateError::Unavailable { detail } if detail.contains("key")),
        "an operator is told what went wrong, in words that are safe to show"
    );
}

#[tokio::test]
async fn connecting_after_a_failure_replaces_it() {
    // The recovery path: the key comes back, the next restore binds, and
    // nothing is left saying the integration is broken.
    let binding = PlatformDesiredState::unconnected();

    binding.unusable("the application's key could not be read").await;
    binding.connect(Arc::new(Connected)).await;

    assert!(binding.is_connected().await);
    assert_eq!(
        binding
            .components("lucentroot")
            .await
            .expect("the repository is connected again"),
        vec!["saas-fabric".to_owned()]
    );
}

#[tokio::test]
async fn disconnecting_after_a_failure_goes_back_to_not_connected() {
    // An operator who gives up and forgets the integration has genuinely not
    // connected one, and should be told so rather than shown a stale failure.
    let binding = PlatformDesiredState::unconnected();

    binding.unusable("the application's key could not be read").await;
    binding.disconnect().await;

    assert_eq!(
        binding
            .components("lucentroot")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
}

// ---------------------------------------------------------------------------
// An unbind waits, and a decision knows what it was read from.
//
// What these pin is the failure that motivated both: a sweep clones repository
// A, an operator disconnects or rebinds to B, and the write in flight lands in
// A anyway — after the platform was explicitly told to stop targeting it. A
// manifest revision cannot see that, because A's manifest did not move.
// ---------------------------------------------------------------------------

/// Everything the tests below want to see the *order* of.
///
/// A shared log rather than two separate assertions, because "the write
/// finished" and "the disconnect returned" are only interesting relative to
/// each other.
type Journal = Arc<Mutex<Vec<String>>>;

/// A repository that records what reached it, and can be held mid-write.
struct Fake {
    /// Which repository this is, so a read can be traced to one of them.
    name: &'static str,

    /// The revision its own reads hand back.
    revision: String,

    /// Every `at` an operation was given, exactly as the adapter saw it.
    seen: Mutex<Vec<String>>,

    /// Held open until a test releases it, when there is one.
    gate: Option<Arc<Notify>>,

    /// Signalled once a write has genuinely started.
    started: Arc<Notify>,

    /// Where a completed write records itself.
    journal: Journal,
}

impl Fake {
    /// A repository that answers immediately.
    fn quick(name: &'static str, revision: &str) -> Arc<Self> {
        Arc::new(Self {
            name,
            revision: revision.to_owned(),
            seen: Mutex::new(Vec::new()),
            gate: None,
            started: Arc::new(Notify::new()),
            journal: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// A repository whose writes block until the returned gate is released.
    fn gated(name: &'static str, revision: &str, journal: &Journal) -> (Arc<Self>, Arc<Notify>) {
        let gate = Arc::new(Notify::new());

        (
            Arc::new(Self {
                name,
                revision: revision.to_owned(),
                seen: Mutex::new(Vec::new()),
                gate: Some(Arc::clone(&gate)),
                started: Arc::new(Notify::new()),
                journal: Arc::clone(journal),
            }),
            gate,
        )
    }

    /// Every `at` this repository has been handed.
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Records an operation and blocks if this repository is gated.
    async fn write(&self, at: &DesiredRevision) {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(at.as_str().to_owned());

        self.started.notify_one();

        if let Some(gate) = &self.gate {
            gate.notified().await;
        }

        self.journal
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(format!("{} wrote", self.name));
    }
}

#[async_trait::async_trait]
impl DesiredState for Fake {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(vec![self.name.to_owned()])
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Ok(ComponentDesired {
            version: crate::Version::parse("0.3.0-preview.1").expect("a version"),
            channel: Channel::Preview,
            policy: UpdatePolicy::Automatic,
            hold: None,
            source: ArtifactSource::Oci {
                repositories: BTreeMap::new(),
            },
            revision: DesiredRevision::new(&self.revision),
        })
    }

    async fn advance(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        at: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        self.write(at).await;

        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &Release,
        _: &crate::Hold,
        at: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        self.write(at).await;

        Ok(())
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &crate::Hold,
        at: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        self.write(at).await;

        Ok(())
    }

    async fn resume(&self, _: &str, _: &str, at: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        self.write(at).await;

        Ok(())
    }
}

/// The hold a pause writes.
fn hold() -> crate::Hold {
    crate::Hold {
        reason: "paused".to_owned(),
        since: "2026-09-04T00:00:00Z".to_owned(),
        note: None,
    }
}

/// Runs an `advance` through the binding, in a task of its own.
fn advancing(
    binding: &Arc<PlatformDesiredState>,
    at: DesiredRevision,
) -> tokio::task::JoinHandle<Result<(), DesiredStateError>> {
    let binding = Arc::clone(binding);

    tokio::spawn(async move {
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &at,
                "Promote",
            )
            .await
    })
}

/// What a test does to the binding while a write is held open.
async fn while_a_write_is_held<F>(
    binding: &Arc<PlatformDesiredState>,
    journal: &Journal,
    gate: &Arc<Notify>,
    started: &Arc<Notify>,
    swap: F,
) where
    F: FnOnce(Arc<PlatformDesiredState>) -> tokio::task::JoinHandle<()>,
{
    let at = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("the repository answers")
        .revision;

    let write = advancing(binding, at);
    started.notified().await;

    let mut swapping = swap(Arc::clone(binding));

    // The swap must still be waiting. This is the whole guarantee: a binding
    // that swapped here would leave the write below landing in a repository
    // this platform had already reported itself as no longer targeting.
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut swapping)
            .await
            .is_err(),
        "changing the binding must wait for the write already in flight"
    );

    gate.notify_one();

    write
        .await
        .expect("the write task must not panic")
        .expect("the write lands in the repository it started against");
    swapping.await.expect("the swap task must not panic");

    assert_eq!(
        journal.lock().unwrap_or_else(PoisonError::into_inner).clone(),
        vec!["A wrote".to_owned(), "swapped".to_owned()],
        "the write finishes first, and the swap returns only after it has"
    );
}

#[tokio::test]
async fn a_disconnect_waits_for_the_write_already_in_flight() {
    let journal: Journal = Arc::new(Mutex::new(Vec::new()));
    let (a, gate) = Fake::gated("A", "sha-a", &journal);
    let started = Arc::clone(&a.started);

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let disconnecting = {
        let journal = Arc::clone(&journal);

        move |binding: Arc<PlatformDesiredState>| {
            tokio::spawn(async move {
                binding.disconnect().await;
                journal
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push("swapped".to_owned());
            })
        }
    };

    while_a_write_is_held(&binding, &journal, &gate, &started, disconnecting).await;

    assert_eq!(a.seen().len(), 1, "the write landed in A, and only once");
    assert_eq!(
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &DesiredRevision::new("sha-a"),
                "Promote",
            )
            .await
            .expect_err("nothing is connected any more"),
        DesiredStateError::NotConnected
    );
}

#[tokio::test]
async fn a_rebind_waits_the_same_way() {
    let journal: Journal = Arc::new(Mutex::new(Vec::new()));
    let (a, gate) = Fake::gated("A", "sha-a", &journal);
    let b = Fake::quick("B", "sha-b");
    let started = Arc::clone(&a.started);

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let rebinding = {
        let journal = Arc::clone(&journal);
        let b = Arc::clone(&b);

        move |binding: Arc<PlatformDesiredState>| {
            tokio::spawn(async move {
                binding.connect(b as Arc<dyn DesiredState>).await;
                journal
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push("swapped".to_owned());
            })
        }
    };

    while_a_write_is_held(&binding, &journal, &gate, &started, rebinding).await;

    assert_eq!(a.seen().len(), 1, "the write in flight landed in A");
    assert!(
        b.seen().is_empty(),
        "and nothing reached B, which was not the repository it was decided against"
    );
    assert_eq!(
        binding.components("lucentroot").await.expect("B answers now"),
        vec!["B".to_owned()],
        "reads after the rebind go to B"
    );
}

#[tokio::test]
async fn a_decision_read_before_a_rebind_is_refused_after_it() {
    // Draining settles what is *in flight*. This is the other half: a decision
    // read a moment ago and written now, across a rebind nobody was blocked on.
    let a = Fake::quick("A", "sha-a");
    let b = Fake::quick("B", "sha-b");

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let decided = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("A answers")
        .revision;

    binding.connect(Arc::clone(&b) as Arc<dyn DesiredState>).await;

    assert_eq!(
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &decided,
                "Promote",
            )
            .await
            .expect_err("this decision was taken about a different repository"),
        DesiredStateError::Conflict
    );
    assert!(
        b.seen().is_empty(),
        "B must not be written to on the strength of a decision read from A"
    );
}

#[tokio::test]
async fn a_decision_read_before_a_disconnect_is_refused_after_a_reconnect_to_the_same_repository() {
    // Same object, and still a different binding. An operator who disconnected
    // and reconnected has made a deliberate statement about what this platform
    // targets; a decision read across it was taken before they made it.
    let a = Fake::quick("A", "sha-a");

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let decided = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("A answers")
        .revision;

    binding.disconnect().await;
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    assert_eq!(
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &decided,
                "Promote",
            )
            .await
            .expect_err("the binding moved on, even though the repository did not"),
        DesiredStateError::Conflict
    );
    assert!(a.seen().is_empty(), "nothing was written");
}

#[tokio::test]
async fn the_adapter_sees_the_revision_it_handed_out() {
    // The tag is the binding's business and nobody else's. An adapter that saw
    // it would have to know to strip it, and a console that saw it would be
    // reading a revision that means something different from the one it was
    // given.
    let a = Fake::quick("A", "sha-a");

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let decided = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("A answers")
        .revision;

    assert_ne!(
        decided.as_str(),
        "sha-a",
        "what leaves the binding carries the generation it was read at"
    );

    binding
        .advance(
            "lucentroot",
            "saas-fabric",
            &Release::Unit(unit()),
            &decided,
            "Promote",
        )
        .await
        .expect("the decision is current");

    assert_eq!(
        a.seen(),
        vec!["sha-a".to_owned()],
        "and what reaches the adapter is its own revision, untagged"
    );
}

#[tokio::test]
async fn a_revision_this_binding_did_not_hand_out_is_refused() {
    // Including the honest mistake: an untagged revision is one that came from
    // somewhere other than a read through this binding, so nothing is known
    // about which repository it describes.
    let a = Fake::quick("A", "sha-a");

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    assert_eq!(
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &DesiredRevision::new("r1"),
                "Promote",
            )
            .await
            .expect_err("this binding never handed that out"),
        DesiredStateError::Conflict
    );
    assert!(a.seen().is_empty(), "nothing was written");
}

#[tokio::test]
async fn pause_and_resume_are_bound_to_the_generation_too() {
    // An operator's own writes, not a sweep's. They go through the same read,
    // and a rebind between the read and the click makes them just as stale.
    let a = Fake::quick("A", "sha-a");
    let b = Fake::quick("B", "sha-b");

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let decided = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("A answers")
        .revision;

    binding.connect(Arc::clone(&b) as Arc<dyn DesiredState>).await;

    assert_eq!(
        binding
            .pause("lucentroot", "saas-fabric", &hold(), &decided, "Pause")
            .await
            .expect_err("read from A, and A is not what is bound"),
        DesiredStateError::Conflict
    );
    assert_eq!(
        binding
            .resume("lucentroot", "saas-fabric", &decided, "Resume")
            .await
            .expect_err("read from A, and A is not what is bound"),
        DesiredStateError::Conflict
    );
    assert!(b.seen().is_empty(), "nothing reached B");

    // And the current generation's revision still works, so this is a
    // staleness check and not a wall.
    let current = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("B answers")
        .revision;

    binding
        .pause("lucentroot", "saas-fabric", &hold(), &current, "Pause")
        .await
        .expect("this decision was read through the binding that is live");

    assert_eq!(b.seen(), vec!["sha-b".to_owned()]);
}

#[tokio::test]
async fn an_operation_outlives_a_caller_that_stopped_waiting() {
    // The hole the drain used to have. An operator's request is cut off at
    // `request_timeout_seconds`, or their browser closes, and axum drops the
    // handler future — which, when the guard lived in that future, released the
    // binding with the write's last request possibly already on the wire. A
    // disconnect could then return, telling the operator the platform had
    // stopped writing to that repository, and the abandoned write land in it.
    let journal: Journal = Arc::new(Mutex::new(Vec::new()));
    let (a, gate) = Fake::gated("A", "sha-a", &journal);
    let started = Arc::clone(&a.started);

    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::clone(&a) as Arc<dyn DesiredState>).await;

    let at = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect("A answers")
        .revision;

    let write = advancing(&binding, at);
    started.notified().await;

    // Exactly what the request timeout does to a handler: the future is gone.
    write.abort();

    let mut disconnecting = {
        let binding = Arc::clone(&binding);

        tokio::spawn(async move { binding.disconnect().await })
    };

    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut disconnecting)
            .await
            .is_err(),
        "the caller went away; the write did not, and the disconnect must still wait for it"
    );

    gate.notify_one();
    disconnecting.await.expect("the disconnect task must not panic");

    assert_eq!(
        journal.lock().unwrap_or_else(PoisonError::into_inner).clone(),
        vec!["A wrote".to_owned()],
        "the write ran to completion even though nobody was waiting for it"
    );
    assert_eq!(a.seen().len(), 1, "and it landed in A exactly once");
    assert_eq!(
        binding
            .advance(
                "lucentroot",
                "saas-fabric",
                &Release::Unit(unit()),
                &DesiredRevision::new("sha-a"),
                "Promote",
            )
            .await
            .expect_err("the disconnect has taken effect"),
        DesiredStateError::NotConnected
    );
}
