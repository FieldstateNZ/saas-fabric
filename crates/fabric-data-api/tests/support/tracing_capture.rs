//! A minimal `tracing::Subscriber` that records event fields, for asserting
//! what this crate logs without pulling in `tracing-subscriber` as a test
//! dependency.

use std::future::Future;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

/// One event's fields, as strings, in the order they were visited. Every
/// `tracing` event this crate emits uses `Display`/string fields, so string
/// comparison is enough for these tests without a richer value type.
pub type CapturedEvent = Vec<(String, String)>;

type Events = Arc<Mutex<Vec<CapturedEvent>>>;

/// Records every event's fields into a shared, growable log.
struct Recording(Events);

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
        let mut fields = Vec::new();
        event.record(&mut FieldVisitor(&mut fields));
        self.0.lock().unwrap().push(fields);
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

/// Runs `future` under a subscriber that records every event's fields,
/// returning the future's output alongside what was captured.
///
/// Held for the whole `.await`: fine for a `#[tokio::test]`, which defaults
/// to a single-threaded runtime, so the thread-local dispatcher this sets
/// stays in effect for every poll of `future`.
pub async fn capture<F: Future>(future: F) -> (F::Output, Vec<CapturedEvent>) {
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let dispatch = tracing::Dispatch::new(Recording(Arc::clone(&events)));
    let _guard = tracing::dispatcher::set_default(&dispatch);

    let output = future.await;
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
