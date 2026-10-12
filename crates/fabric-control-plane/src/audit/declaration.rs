//! The audit records a convergence pass appends around each identity
//! client write: the declaration ledger (D01-15).

use fabric_client_model::{ClientId, OidcClient, RealmName};
use fabric_core::{event_id, EventType};

use crate::{AuditRecord, AuditStream, DOMAIN_ID};

/// Which side of a provider write a declaration record is (D01-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclarationStage {
    /// Before the provider is asked.
    Writing,
    /// Once the provider confirmed.
    Written,
}

impl DeclarationStage {
    /// The event each stage is recorded as.
    pub(crate) const fn event(self) -> &'static str {
        match self {
            Self::Writing => "control_plane.audit.identity_declaration_writing",
            Self::Written => "control_plane.audit.identity_declaration_written",
        }
    }
}

/// A pass acting as `operator` is about to write, or has written, one
/// declared identity client (D01-15). The declaration ledger reads these
/// back as the baseline a missing callback is classified against.
///
/// `callbacks` is the SHA-256 of each declared callback, comma-joined:
/// the record names a callback by digest, never by value.
pub(crate) fn identity_declaration(
    operator: &str,
    client: &ClientId,
    realm: &RealmName,
    declared: &OidcClient,
    stage: DeclarationStage,
) -> AuditRecord {
    const OPERATION: &str = "converge_client_identity";
    let event = stage.event();
    let callbacks = fabric_reconciliation::callback_digests(declared)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let number = match stage {
        DeclarationStage::Writing => 20,
        DeclarationStage::Written => 21,
    };
    tracing::info!(
        event = event,
        event_id = event_id(DOMAIN_ID, EventType::Success, number),
        operation = OPERATION,
        requested_by = operator,
        client_id = %client,
        realm = %realm,
        identity_client = %declared.id,
        strategy = %declared.redirect.kind(),
        "a convergence pass recorded an identity declaration write"
    );
    AuditRecord::new(AuditStream::Client(client.clone()), event, OPERATION, operator)
        .with("realm", realm)
        .with("identity_client", &declared.id)
        .with("strategy", declared.redirect.kind())
        .with("callbacks", callbacks)
}
