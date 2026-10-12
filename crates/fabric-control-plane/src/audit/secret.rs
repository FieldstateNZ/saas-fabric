//! The audit record for an act on a client's secret.

use fabric_client_model::ClientId;
use fabric_core::{event_id, EventType};

use crate::{AuditRecord, AuditStream, Operator, DOMAIN_ID};

/// An operator did something to one client's secret.
///
/// # What is recorded, and what is not
///
/// The actor, the client, the path, the operation, and how it turned out —
/// including the version, because "which version did they replace" is the
/// question an incident asks first.
///
/// **Never a value, and never a key name.** A path is a name an operator
/// chose; a value is the secret itself, and a key name can be almost as
/// telling. Nothing that could contain either reaches this function: it is
/// handed a validated path and an outcome, and there is no parameter through
/// which a value could arrive even by mistake.
///
/// Reveal is recorded like any other operation. Reading a secret is an act,
/// not a side effect of looking at a page, and it is the act an investigation
/// most wants to see.
pub(crate) fn client_secret(
    operator: &Operator,
    client: &ClientId,
    path: &crate::SecretPath,
    operation: &'static str,
    outcome: &str,
    version: Option<u64>,
) -> AuditRecord {
    const EVENT: &str = "control_plane.audit.client_secret";
    tracing::info!(
        event = EVENT,
        event_id = event_id(DOMAIN_ID, EventType::Success, 3),
        operation,
        outcome,
        requested_by = operator.subject(),
        client_id = %client,
        secret_path = %path,
        version,
        "operator acted on a client's secret"
    );
    AuditRecord::new(
        AuditStream::Client(client.clone()),
        EVENT,
        operation,
        operator.subject(),
    )
    .with("outcome", outcome)
    .with("secret_path", path)
    .with("version", version.map(|v| v.to_string()).unwrap_or_default())
}
