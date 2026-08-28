//! A minimal `tracing::Subscriber` that records event fields, for asserting
//! what this crate logs without pulling in `tracing-subscriber` as a test
//! dependency.
//!
//! # Why the subscriber is global and always enabled
//!
//! `tracing` caches each callsite's `Interest` the first time that callsite
//! fires, and computes it from *the firing thread's* dispatcher. Tests run in
//! parallel, so a test that captures nothing could be the first to reach, say,
//! `data_api.unknown_tenant_probed`, have "never interested" cached for the
//! whole process, and silently disable that event for a capturing test running
//! beside it. Scoping a subscriber to the capturing thread — the obvious
//! design, and the one this module used to have — loses that race roughly one
//! run in twenty-five.
//!
//! So the subscriber is installed once as the global default and stays: no
//! callsite is ever registered against `NoSubscriber`, the cached answer is
//! always "dispatch it", and a thread-local sink decides whether the event is
//! actually recorded. [`install`] is called from the shared app builder, so it
//! has run before any test issues a request.

use std::cell::RefCell;
use std::future::Future;
use std::sync::{Arc, Mutex, Once};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

/// One event's fields, as strings, in the order they were visited. Every
/// `tracing` event this crate emits uses `Display`/string fields, so string
/// comparison is enough for these tests without a richer value type.
pub type CapturedEvent = Vec<(String, String)>;

type Events = Arc<Mutex<Vec<CapturedEvent>>>;

thread_local! {
    /// Where [`Recording`] puts events while [`capture`] is running on this
    /// thread, and `None` everywhere else — which is what makes an
    /// always-enabled global subscriber free for the tests that assert on no
    /// logs at all.
    static SINK: RefCell<Option<Events>> = const { RefCell::new(None) };
}

/// Records every event's fields into whatever sink the current thread has set.
struct Recording;

impl Subscriber for Recording {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        // The `Arc` is cloned out of the borrow so nothing is held across the
        // field visit.
        let Some(events) = SINK.with(|sink| sink.borrow().clone()) else {
            return;
        };

        let mut fields = Vec::new();
        event.record(&mut FieldVisitor(&mut fields));
        events.lock().unwrap().push(fields);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

struct FieldVisitor<'a>(&'a mut Vec<(String, String)>);

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }
}

/// Installs the always-enabled global subscriber, once per test binary.
///
/// Called from the shared app builder rather than from [`capture`] alone: by
/// the time any test can emit an event it has built an app, so this has run
/// and no callsite can be registered against an absent subscriber. See the
/// module docs for what goes wrong otherwise.
pub fn install() {
    static INSTALLED: Once = Once::new();

    INSTALLED.call_once(|| {
        // Only fails if something else already claimed the global default,
        // which nothing in this suite does.
        let _ = tracing::subscriber::set_global_default(Recording);
    });
}

/// Runs `future` under a sink that records every event's fields, returning the
/// future's output alongside what was captured.
///
/// The sink is thread-local, which is fine for a `#[tokio::test]`: that
/// defaults to a single-threaded runtime, so every poll of `future` happens on
/// the thread this sets.
pub async fn capture<F: Future>(future: F) -> (F::Output, Vec<CapturedEvent>) {
    install();

    let events: Events = Arc::new(Mutex::new(Vec::new()));
    SINK.with(|sink| *sink.borrow_mut() = Some(Arc::clone(&events)));

    let output = future.await;

    SINK.with(|sink| *sink.borrow_mut() = None);
    let captured = events.lock().unwrap().clone();

    (output, captured)
}

/// Finds a field's value on the first captured event with the given `event`
/// name, if any such event was captured.
pub fn field_value<'a>(events: &'a [CapturedEvent], event_name: &str, field_name: &str) -> Option<&'a str> {
    events
        .iter()
        .find(|fields| {
            fields
                .iter()
                .any(|(name, value)| name == "event" && value == event_name)
        })
        .and_then(|fields| fields.iter().find(|(name, _)| name == field_name))
        .map(|(_, value)| value.as_str())
}
