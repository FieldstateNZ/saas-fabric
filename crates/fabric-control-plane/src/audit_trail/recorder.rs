//! Handing each audit record to the trail, when there is one.

use std::sync::Arc;

use fabric_core::{event_id, Clock, EventType};

use crate::audit_trail::{AuditEntry, AuditError, AuditRecord, AuditTrail};
use crate::DOMAIN_ID;

/// What every audited operation hands its record to.
///
/// Concrete, and cheap to clone: one is built at the composition root and
/// shared by every service that audits. It holds the trail as an `Option`
/// because storage activation is a deferred decision (ADR 0028): a
/// deployment that states none records to the log pipeline only, as before
/// this existed, and says so once at startup.
#[derive(Clone)]
pub struct AuditRecorder {
    trail: Option<Arc<dyn AuditTrail>>,
    clock: Arc<dyn Clock>,
}

impl AuditRecorder {
    /// A recorder with no durable trail: records reach the log pipeline only.
    #[must_use]
    pub fn disabled(clock: Arc<dyn Clock>) -> Self {
        Self { trail: None, clock }
    }

    /// A recorder appending every record to `trail`.
    #[must_use]
    pub fn durable(trail: Arc<dyn AuditTrail>, clock: Arc<dyn Clock>) -> Self {
        Self {
            trail: Some(trail),
            clock,
        }
    }

    /// The trail, when this deployment keeps one: the store the declaration
    /// ledger reads and appends to (D01-15).
    pub(crate) fn trail(&self) -> Option<Arc<dyn AuditTrail>> {
        self.trail.clone()
    }

    /// The clock records are stamped with.
    pub(crate) fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock)
    }

    /// Appends `record` to the trail, if there is one.
    ///
    /// # Why a failure does not fail the operation
    ///
    /// This runs after the change it records has been written: the record
    /// carries the revision the write produced. Failing the request then
    /// would tell an operator a change that landed did not, and invite a
    /// retry that makes a second one. So a failure is logged at `error` with
    /// the event name and stream — the event's own log line, already emitted,
    /// keeps its content in the log pipeline — and never silently dropped.
    ///
    /// # Why in a task of its own
    ///
    /// A request can be cancelled at any await. The append, and the log line
    /// reporting how it ended, run in a spawned task the caller only awaits,
    /// so a disconnect can stop neither: not between writing an entry and
    /// recording the stream's new head, and not before a failure is logged
    /// or a success's head is anchored in the log pipeline.
    pub(crate) async fn record(&self, record: AuditRecord) {
        let Some(trail) = self.trail.clone() else {
            return;
        };
        let at = self.clock.now_unix_seconds();
        let event = record.event().to_owned();
        let stream = record.stream().to_string();
        let reported = (event.clone(), stream.clone());
        let task = tokio::spawn(async move {
            let appended = trail.append(record, at).await;
            report(&event, &stream, appended);
        });

        // Only a panic in the append, or the runtime shutting down, reaches
        // this; the task reported nothing, so the failure is logged here.
        if task.await.is_err() {
            let (event, stream) = reported;
            failed(
                &event,
                &stream,
                &AuditError::Unavailable {
                    detail: "the append did not complete".to_owned(),
                },
            );
        }
    }
}

/// Logs how an append ended. A success's sequence and hash are a second
/// anchor for the stream's head, outside the store.
fn report(event: &str, stream: &str, appended: Result<AuditEntry, AuditError>) {
    match appended {
        Ok(entry) => tracing::info!(
            event = "control_plane.audit.appended",
            event_id = event_id(DOMAIN_ID, EventType::Success, 18),
            audited = %event,
            stream = %stream,
            sequence = entry.sequence(),
            hash = %entry.hash(),
            "audit record appended to the durable trail"
        ),
        Err(error) => failed(event, stream, &error),
    }
}

/// Records that an append was refused or failed. An oversize refusal gets
/// its own event so it can be found without parsing error text.
fn failed(event: &str, stream: &str, error: &AuditError) {
    let (name, number) = match error {
        AuditError::EntryTooLarge { .. } => ("control_plane.audit.refused", 6),
        _ => ("control_plane.audit.append_failed", 5),
    };
    tracing::error!(
        event = name,
        event_id = event_id(DOMAIN_ID, EventType::Error, number),
        audited = %event,
        stream = %stream,
        error = %error,
        "an audit record did not reach the durable trail"
    );
}
