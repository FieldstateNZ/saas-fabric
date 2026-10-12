//! The declaration ledger over the durable audit trail (D01-15).
//!
//! # Why the audit trail
//!
//! Reconciliation needs a record of what Fabric wrote that nobody else can
//! write: not desired-state bytes, not provider metadata, not a process
//! cache (see `fabric_reconciliation`'s `ledger` module). The audit trail is
//! the one durable store the control plane already has with that property:
//! append-only, hash-chained per client stream, verified on read. Recording
//! the baseline there rather than in a second store keeps one history of
//! what happened to a client, which is also what O5's revert work reads, and
//! means a Git revert to earlier bytes — which can bring an earlier revision
//! back (#159) — cannot bring an earlier baseline back with it: the baseline
//! is bound to the realm and each callback's digest, not to a revision.
//!
//! A deployment with no trail has no ledger. Reconciliation then knows no
//! baseline, and every ambiguous transition is held until re-asserted.
//!
//! # Reading
//!
//! The client's stream is read whole and verified against its recorded head;
//! a stream that does not verify is unreadable, never partly trusted. Records
//! for another realm are not this identity instance's and are skipped.
//!
//! In the 121–150 line band: one adapter, its two port methods, and the
//! constructor that attaches it to every reconciler; splitting the
//! constructor away would separate "a pass keeps a ledger" from the ledger.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use fabric_client_model::{Client, OidcClient, OidcClientId};
use fabric_core::Clock;
use fabric_reconciliation::{
    AppliedBaseline, CallbackDigest, DeclarationLedger, IdentityReconciler, LedgerError, WriteStage,
};

use crate::audit::{identity_declaration, DeclarationStage};
use crate::state::ControlPlaneState;
use crate::{AuditStream, AuditTrail, ControlPlaneError, Operator};

/// See this module's rustdoc.
pub(crate) struct AuditDeclarationLedger {
    trail: Arc<dyn AuditTrail>,
    clock: Arc<dyn Clock>,
    /// The operator whose authority the pass runs under (ADR 0012).
    operator: String,
}

/// A reconciler acting as this operator, keeping its ledger in the audit
/// trail when this deployment has one, or the reason there is none.
///
/// Every path into the identity provider starts here (ADR 0012): the
/// platform holds no credential of its own, so a pass without an operator
/// to authorise it cannot be constructed.
pub(crate) fn reconciler(
    state: &ControlPlaneState,
    operator: &Operator,
) -> Result<IdentityReconciler, ControlPlaneError> {
    let factory = state
        .identity_provider
        .as_ref()
        .ok_or(ControlPlaneError::ConvergenceUnavailable)?;
    let reconciler = IdentityReconciler::new(factory.acting_as(operator.token()));
    let audit = state.service.audit();

    Ok(match audit.trail() {
        Some(trail) => reconciler.with_ledger(Arc::new(AuditDeclarationLedger {
            trail,
            clock: audit.clock(),
            operator: operator.subject().to_owned(),
        })),
        None => reconciler,
    })
}

#[async_trait]
impl DeclarationLedger for AuditDeclarationLedger {
    async fn read(&self, client: &Client) -> Result<AppliedBaseline, LedgerError> {
        let unreadable = |detail: String| LedgerError::Unreadable { detail };
        let stored = self
            .trail
            .read(&AuditStream::Client(client.id.clone()))
            .await
            .map_err(|error| unreadable(error.to_string()))?;
        stored.verify().map_err(|error| unreadable(error.to_string()))?;

        let mut baseline = AppliedBaseline::unknown();
        for entry in &stored.entries {
            let record = entry.record();
            let stage = match record.event() {
                event if event == DeclarationStage::Writing.event() => WriteStage::Writing,
                event if event == DeclarationStage::Written.event() => WriteStage::Written,
                _ => continue,
            };
            if record.field("realm") != Some(client.identity.realm.as_str()) {
                continue;
            }
            let declared = record
                .field("identity_client")
                .ok_or_else(|| unreadable("a declaration record names no identity client".to_owned()))
                .and_then(|id| OidcClientId::try_new(id).map_err(|error| unreadable(error.to_string())))?;
            let callbacks = record
                .field("callbacks")
                .unwrap_or_default()
                .split(',')
                .filter(|digest| !digest.is_empty())
                .map(CallbackDigest::parse)
                .collect::<Result<BTreeSet<_>, _>>()
                .map_err(unreadable)?;
            baseline.replay(declared, callbacks, stage);
        }
        Ok(baseline)
    }

    async fn record(
        &self,
        client: &Client,
        declared: &OidcClient,
        stage: WriteStage,
    ) -> Result<(), LedgerError> {
        let stage = match stage {
            WriteStage::Writing => DeclarationStage::Writing,
            WriteStage::Written => DeclarationStage::Written,
        };
        let record = identity_declaration(
            &self.operator,
            &client.id,
            &client.identity.realm,
            declared,
            stage,
        );

        self.trail
            .append(record, self.clock.now_unix_seconds())
            .await
            .map(|_| ())
            .map_err(|error| LedgerError::Unrecorded {
                detail: error.to_string(),
            })
    }
}
