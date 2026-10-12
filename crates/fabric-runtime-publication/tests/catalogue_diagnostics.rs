//! Review of #170: a refused catalogue is reported without its content. A
//! running replica without a copy refuses a generation whose catalogue
//! matches its digest but carries a malformed value, keeps its whole view,
//! counts the held pass and names the catalogue, and neither the readiness
//! detail (`last_hold`) nor any log event carries the value.
//!
//! Its own test binary, because it installs a global log subscriber: the
//! refresher logs from a background task, which a thread-local capture would
//! not see.

// `support` is Unix-only (`std::os::unix::fs::MetadataExt`).
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use fabric_runtime_publication::{RuntimePublication as _, CATALOG_FILE};
use fabric_tenant_runtime::build_published_runtime;
use serde_json::json;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

const SENTINEL: &str = "SYNTHETIC_SECRET_170_NOT_REAL";

/// Every event any thread logs, each as its rendered fields.
static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Everything;

impl Subscriber for Everything {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut rendered = Rendered(String::new());
        event.record(&mut rendered);
        EVENTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(rendered.0);
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

struct Rendered(String);

impl Visit for Rendered {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

#[tokio::test]
async fn a_refused_catalogue_value_reaches_neither_the_hold_nor_the_log() {
    tracing::subscriber::set_global_default(Everything).unwrap();

    let dir = support::TempDir::new();
    dir.publisher().publish(&support::base_snapshot(1)).await.unwrap();
    let (source, loads) = support::CountingPublished::over(support::published_files(&dir));
    let runtime = build_published_runtime(&support::runtime_config(), Arc::new(source))
        .await
        .unwrap();
    let served = runtime.resolver.served_generation();
    assert!(served.is_some());

    dir.publisher().publish(&support::base_snapshot(2)).await.unwrap();
    support::edit_json(&dir, CATALOG_FILE, |catalog| {
        catalog["articles"]["operations"] = json!([SENTINEL]);
    });
    support::rewrite_record(&dir, 2, |_| {});

    let before = loads.load(std::sync::atomic::Ordering::SeqCst);
    runtime.handles.refresh_now();
    support::poll_for_load_count_above(&loads, before, Duration::from_secs(5)).await;

    assert_eq!(
        runtime.resolver.served_generation(),
        served,
        "the whole view still serves"
    );
    let report = runtime.status.report();
    assert_eq!(report.held_passes, 1);
    let last_hold = report.last_hold.unwrap();
    assert!(last_hold.contains("catalog"), "names the document: {last_hold}");
    assert!(
        last_hold.contains("does not match the catalogue shape at line"),
        "names a category and a location: {last_hold}"
    );
    assert!(!last_hold.contains(SENTINEL), "{last_hold}");

    let events = EVENTS.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert!(
        events
            .iter()
            .any(|event| event.contains("runtime.generation_refused") && event.contains("catalog")),
        "the hold is logged: {events:#?}"
    );
    for event in &events {
        assert!(!event.contains(SENTINEL), "{event}");
    }
    runtime.handles.shutdown().await.unwrap();
}
