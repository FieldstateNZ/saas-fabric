//! Handing each audit record to the trail, when there is one.

use std::sync::Arc;

use fabric_core::{event_id, Clock, EventType};

use crate::audit_trail::{AuditError, AuditRecord, AuditTrail};
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
    /// A request can be cancelled at any await. The append runs in a spawned
    /// task the caller only awaits, so a disconnect cannot stop it between
    /// writing an entry and recording the stream's new head.
    pub(crate) async fn record(&self, record: AuditRecord) {
        let Some(trail) = self.trail.clone() else {
            return;
        };
        let at = self.clock.now_unix_seconds();
        let event = record.event().to_owned();
        let stream = record.stream().to_string();
        let appended = tokio::spawn(async move { trail.append(record, at).await }).await;

        match appended {
            Ok(Ok(entry)) => tracing::info!(
                event = "control_plane.audit.appended",
                event_id = event_id(DOMAIN_ID, EventType::Success, 18),
                audited = %event,
                stream = %stream,
                sequence = entry.sequence(),
                hash = %entry.hash(),
                "audit record appended to the durable trail"
            ),
            Ok(Err(error)) => failed(&event, &stream, &error),
            Err(_) => failed(
                &event,
                &stream,
                &AuditError::Unavailable {
                    detail: "the append did not complete".to_owned(),
                },
            ),
        }
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
