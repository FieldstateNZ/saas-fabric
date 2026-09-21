//! Surviving a panic inside one scheduled tick, without ending the loop
//! that scheduled it.
//!
//! # Why the host owns this, and not the rules crate
//!
//! `fabric-platform-management` does not catch panics, and should not: a
//! panic there is a bug in that crate's own logic, and swallowing it would
//! hide the bug rather than fix it. What this module protects the loop
//! against is different -- an *adapter* the host wired in (a registry
//! client, a Git host, a publication target) panicking on a bad response --
//! and whether a scheduled loop survives that is exactly the same kind of
//! decision as its cadence: both are the deployment's scheduling policy, not
//! the rules crate's. That is the same reason `start_sweeping` and
//! `start_publishing` already live here rather than there.
//!
//! # Why this lives beside `platform`, not inside it
//!
//! `startup::platform::sweeping::start_sweeping` and
//! `startup::platform::publishing::start_publishing` are two of its callers;
//! `startup::operator_keys::refresh::spawn` -- the operator signing-key
//! refresh loop -- is the third. It had the identical shape and the
//! identical flaw: one loop task awaited its pass directly, so a panic there
//! ended key refresh for good, the same way a panic used to end a sweep or a
//! publication pass, until it was moved onto this helper too. That third
//! caller living in `startup::operator_keys` rather than
//! `startup::platform` is why `survive_panic` is `pub(in crate::startup)`
//! rather than `pub(super)` and this file sits in `startup`, not
//! `startup::platform`.

use std::future::Future;

/// Runs one tick's `work`, surviving a panic without ending the loop that
/// called this.
///
/// # Why a task per tick, not `catch_unwind`
///
/// `std::panic::catch_unwind` requires its argument to be `UnwindSafe`, and
/// an arbitrary `async` block borrowed from a loop body is not one without
/// wrapping every capture in `AssertUnwindSafe` -- bound gymnastics that buy
/// nothing here, because `tokio::spawn` already gives the same guarantee for
/// free: a panic inside a spawned task is reported at its `JoinHandle` as an
/// `Err` rather than propagating into the task that awaits it. Whatever runs
/// on `Drop` during the unwind -- a running flag's guard releasing itself,
/// for instance -- runs exactly the same way regardless of which of the two
/// stops the unwind, since unwinding itself does not know or care which
/// boundary catches it.
///
/// # What each outcome does
///
/// - `Ok(())`: the tick completed. Nothing to do.
/// - `Err(error) if error.is_panic()`: logged as a fixed sentence, and
///   deliberately never the join error's `Display` or the panic payload
///   itself. A `JoinError`'s `Display` can carry the panic message
///   verbatim, and a panic raised inside an adapter this host did not write
///   can carry anything an upstream response put in it, including a
///   credential -- the same reason `SafeDiagnostic` exists in the rules
///   crate, applied here because a panic message never passes through that
///   type at all. No `event_id` is attached: unlike `fabric-control-plane`
///   and its sibling domain crates, this crate has never assigned itself a
///   `DOMAIN_ID` (every other crate's is `pub(crate)`, so there is none to
///   borrow), and `main.rs`'s own two events carry none either -- inventing
///   one for a single log line here would not match anything else in this
///   crate.
/// - `Err(_)` (cancelled): only happens when the runtime is tearing this
///   task down out from under it, at shutdown. There is no schedule left to
///   continue, so nothing is logged.
///
/// # What this does not protect against
///
/// Dropping a `JoinHandle` detaches its task rather than aborting it, so a
/// tick already in flight keeps running to completion even if the loop task
/// that spawned it were itself aborted. Nothing aborts a loop task today --
/// this is latent, not observed -- but it is why an abort would not also
/// stop the tick in progress.
///
/// The whole mechanism depends on `panic = "unwind"`. Nothing in this
/// workspace's root `Cargo.toml` sets a profile to `panic = "abort"`, and
/// this crate does not set one either; if either ever did, a panic would
/// abort the process before `tokio::spawn`'s `JoinHandle` could report
/// anything, and this module would have nothing left to catch.
pub(in crate::startup) async fn survive_panic<F>(pass: &'static str, work: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    match tokio::spawn(work).await {
        Err(error) if error.is_panic() => {
            tracing::error!(
                event = "control_plane.scheduled_pass_panicked",
                pass,
                "a scheduled pass panicked; the schedule continues"
            );
        }
        // `Ok(())` (completed) and `Err(_)` (cancelled at shutdown) both
        // leave nothing to do -- see this function's own rustdoc for why
        // each is unremarkable on its own.
        Ok(()) | Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, Once, PoisonError};

    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::Layer;

    use super::survive_panic;

    #[tokio::test]
    async fn a_panicking_tick_is_followed_by_a_running_one() {
        // The spawned task's panic prints a backtrace to stderr even though
        // this test passes -- that is `tokio::spawn` reporting the panic the
        // way it always does, not a sign anything here needs fixing.
        survive_panic("test", async { panic!("boom") }).await;

        let ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran);
        survive_panic("test", async move {
            flag.store(true, Ordering::SeqCst);
        })
        .await;

        assert!(
            ran.load(Ordering::SeqCst),
            "a tick after a panicking one must still run"
        );
    }

    /// One event's fields, as a name and its `Debug`-formatted value.
    /// `Debug`, not `Display`, because `Visit::record_debug` is the only
    /// method this implements -- every other field type's default
    /// implementation delegates to it, so this one method captures a
    /// field regardless of whether it was recorded as a string, an
    /// integer, or anything else.
    #[derive(Debug, Default)]
    struct RecordedEvent {
        fields: Vec<(String, String)>,
    }

    impl tracing::field::Visit for RecordedEvent {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.fields.push((field.name().to_owned(), format!("{value:?}")));
        }
    }

    thread_local! {
        /// Where this thread's events go while a test has opted in, and
        /// `None` on every thread that has not -- which is every thread but
        /// the one running a capturing test, including every other test in
        /// this binary that happens to run at the same time.
        ///
        /// This is why a capturing test's own `survive_panic` call must run
        /// on the same OS thread that set this: `#[tokio::test]`'s default
        /// `current_thread` runtime does, since it drives the whole test
        /// body -- spawned tasks included -- on the thread that called it. A
        /// `#[tokio::test(flavor = "multi_thread")]` test would poll the
        /// spawned tick on a worker thread instead, where `SINK` reads back
        /// `None`, and the event would go uncaptured rather than fail loudly.
        static SINK: RefCell<Option<Arc<Mutex<Vec<RecordedEvent>>>>> = const { RefCell::new(None) };
    }

    /// Routes each event to whichever thread's [`SINK`] is set, so tests
    /// running in parallel each see only their own.
    ///
    /// # Why a global default, when the rest of this file needs none
    ///
    /// The obvious approach -- `tracing::subscriber::set_default`, scoped to
    /// one test -- races every *other* test in this binary that also hits
    /// `survive_panic`'s `tracing::error!` callsite. Tracing caches, once per
    /// callsite for the whole process, whether *any* currently active
    /// subscriber is interested; a hit from a thread with no override can
    /// cache "never" moments before this thread's override would have
    /// answered differently, and that cached "never" then skips the event on
    /// every thread, including this one.
    ///
    /// Installing one real subscriber globally, exactly once, removes the
    /// two-possible-answers window the cache is racing on. The repair is not
    /// `set_global_default` itself, though -- it is what happens on the way
    /// there: `tracing::subscriber::set_global_default` converts its
    /// argument with `Dispatch::new`, and `Dispatch::new` calls
    /// `callsite::register_dispatch`, which walks every callsite already
    /// registered in the process and recomputes its interest against the
    /// dispatchers now in play (`tracing-core` 0.1.36, `dispatcher.rs` /
    /// `callsite.rs` -- the exact mechanism, so worth naming the version
    /// pinned in this workspace's `Cargo.lock`, since a future one could
    /// change it). Routing through a thread-local inside the layer is what
    /// keeps the capture scoped to one test despite the subscriber itself
    /// being global.
    struct CapturingLayer;

    impl<S: tracing::Subscriber> Layer<S> for CapturingLayer {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            SINK.with(|sink| {
                let sink = sink.borrow();
                if let Some(events) = sink.as_ref() {
                    let mut recorded = RecordedEvent::default();
                    event.record(&mut recorded);
                    events
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(recorded);
                }
            });
        }
    }

    #[tokio::test]
    async fn a_panicking_ticks_log_line_carries_no_credential() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            let subscriber = tracing_subscriber::registry().with(CapturingLayer);
            // No other test in this crate's unit tests installs a global
            // default -- if one ever does (a `telemetry::init` call reused
            // in a test, say), it must lose this race loudly, here, rather
            // than leave `SINK` looking empty and this test failing with no
            // clue why.
            tracing::subscriber::set_global_default(subscriber)
                .expect("this test installs the only global subscriber in this binary");
        });

        let events = Arc::new(Mutex::new(Vec::new()));
        SINK.with(|sink| *sink.borrow_mut() = Some(Arc::clone(&events)));

        // The spawned task's panic prints a backtrace to stderr even though
        // this test passes -- that is `tokio::spawn` reporting the panic the
        // way it always does, not a sign anything here needs fixing.
        survive_panic("test", async { panic!("secret-token-XYZ") }).await;

        SINK.with(|sink| *sink.borrow_mut() = None);

        let recorded = events.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            recorded.len(),
            1,
            "expected exactly one logged event, saw {recorded:?}"
        );
        let panic_event = &recorded[0];

        // Exactly these three: `message` (the fixed sentence), `pass`, and
        // `event`. An extra field -- `payload`, `error`, anything -- is
        // exactly how a panic's contents would leak, so this is checked by
        // name, not just by value.
        let names: BTreeSet<&str> = panic_event.fields.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            BTreeSet::from(["message", "pass", "event"]),
            "only these fields belong on a scheduled-pass panic; an unexpected one may be a payload"
        );

        for (name, value) in &panic_event.fields {
            assert!(
                !value.contains("secret-token"),
                "field {name:?} carried what looks like a credential: {value:?}"
            );
        }
    }
}
