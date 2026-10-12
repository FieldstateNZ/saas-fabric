//! A requester cancelled while its append is in flight: the append still
//! finishes, and how it ended is still logged.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use fabric_core::SystemClock;
use tokio::sync::oneshot;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::Layer;

use super::*;

/// How the gated append ends once released.
#[derive(Clone, Copy)]
enum Ending {
    Appended,
    Failed,
    Refused,
}

/// A trail whose append says it has started, then waits to be released.
struct Gated {
    started: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
    ending: Ending,
}

#[async_trait]
impl AuditTrail for Gated {
    async fn append(&self, record: AuditRecord, at: u64) -> Result<AuditEntry, AuditError> {
        let started = self.started.lock().unwrap().take().unwrap();
        let release = self.release.lock().unwrap().take().unwrap();
        started.send(()).unwrap();
        release.await.unwrap();
        match self.ending {
            Ending::Appended => AuditEntry::seal(None, at, record),
            Ending::Failed => Err(AuditError::Unavailable {
                detail: "injected".to_owned(),
            }),
            Ending::Refused => Err(AuditError::EntryTooLarge { size: 1, limit: 0 }),
        }
    }

    async fn read(&self, stream: &AuditStream) -> Result<StoredStream, AuditError> {
        InMemoryAuditTrail::new().read(stream).await
    }

    fn describe(&self) -> String {
        "a gated test trail".to_owned()
    }
}

/// Collects the `event` field of every event on this thread.
struct Capture(Arc<Mutex<Vec<String>>>);

struct EventName(Option<String>);

impl tracing::field::Visit for EventName {
    fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {}

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "event" {
            self.0 = Some(value.to_owned());
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut name = EventName(None);
        event.record(&mut name);
        if let Some(name) = name.0 {
            self.0.lock().unwrap_or_else(PoisonError::into_inner).push(name);
        }
    }
}

/// Cancels the requester once its append has started, releases the append,
/// and returns the events logged. A current-thread runtime runs the detached
/// task on this thread, inside the capture.
async fn logged_after_cancelling_the_requester(ending: Ending) -> Vec<String> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let _capture =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(Capture(Arc::clone(&events))));
    let (started, has_started) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let trail = Gated {
        started: Mutex::new(Some(started)),
        release: Mutex::new(Some(released)),
        ending,
    };
    let recorder = AuditRecorder::durable(Arc::new(trail), Arc::new(SystemClock));
    let record = AuditRecord::new(
        AuditStream::Catalogue,
        "control_plane.audit.test",
        "test",
        "operator",
    );

    let requester = tokio::spawn(async move { recorder.record(record).await });
    has_started.await.unwrap();
    requester.abort();
    assert!(requester.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();

    for _ in 0..100 {
        if !events.lock().unwrap().is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let logged = events.lock().unwrap().clone();
    logged
}

#[tokio::test]
async fn a_success_is_anchored_in_the_log_after_its_requester_is_cancelled() {
    let logged = logged_after_cancelling_the_requester(Ending::Appended).await;
    assert_eq!(logged, ["control_plane.audit.appended"]);
}

#[tokio::test]
async fn a_failure_is_logged_after_its_requester_is_cancelled() {
    let logged = logged_after_cancelling_the_requester(Ending::Failed).await;
    assert_eq!(logged, ["control_plane.audit.append_failed"]);
}

#[tokio::test]
async fn a_refusal_is_logged_after_its_requester_is_cancelled() {
    let logged = logged_after_cancelling_the_requester(Ending::Refused).await;
    assert_eq!(logged, ["control_plane.audit.refused"]);
}
