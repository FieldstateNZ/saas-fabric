//! Choosing where audit records go.

use std::sync::Arc;

use fabric_control_plane::{AuditRecorder, AuditTrail};
use fabric_core::Clock;

use crate::config::AuditTrailConfig;
use crate::local_audit_trail::LocalAuditTrail;

/// Builds the recorder every audited operation hands its record to.
///
/// Disabled is a stated mode, not a silent one: startup logs that records
/// reach the log pipeline only, so nobody reads a deployment's audit posture
/// as more than it is.
///
/// # Errors
///
/// A message if a stated local directory cannot be opened — including one
/// holding a stream that does not verify. A trail that has been tampered
/// with is refused at startup rather than extended.
pub(super) fn recorder(config: &AuditTrailConfig, clock: &Arc<dyn Clock>) -> Result<AuditRecorder, String> {
    match config {
        AuditTrailConfig::Disabled => {
            tracing::info!(
                event = "control_plane.audit_trail_disabled",
                "no durable audit trail; audit records reach the log pipeline only \
                 (storage activation is deferred, ADR 0028)"
            );
            Ok(AuditRecorder::disabled(Arc::clone(clock)))
        }
        AuditTrailConfig::LocalDirectory { path } => {
            let trail = LocalAuditTrail::open(path).map_err(|error| error.to_string())?;
            tracing::warn!(
                event = "control_plane.development_audit_trail",
                trail = %trail.describe(),
                "using a local development audit trail"
            );
            Ok(AuditRecorder::durable(Arc::new(trail), Arc::clone(clock)))
        }
    }
}
