//! Where a pass finds the declaration ledger for one client (D01-15).
//!
//! # Partial: no durable adapter exists
//!
//! The ledger has to be trusted provenance of the last successful
//! projection, bound to the client's identity instance and revision:
//! never desired-state Git bytes, which anyone with write access edits and a
//! revert can replay; never the provider's current fields, which a hand edit
//! changes; never a process cache, which a restart empties; and not the audit
//! trail, which is written after a mutation and does not fail it (#108
//! comment 6119161317). The store that qualifies is the protected
//! continuity state that disposition calls for — durable, with revision,
//! check-and-set and reservation semantics — and it does not exist yet.
//!
//! So a deployment is built with no [`DeclarationLedgers`]. Reconciliation
//! then knows no baseline: a converged client stays converged, and any move
//! that leaves a declared callback missing is held whole — before any old
//! callback is removed — until an operator's audited re-assertion writes it.
//! Tests hand in an in-memory implementation to prove the classification;
//! it is process-only and certifies nothing about restarts.

use std::sync::Arc;

use fabric_client_model::{ClientId, ClientRevision};
use fabric_reconciliation::{DeclarationLedger, IdentityReconciler};

use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// Hands each pass the ledger for one client, scoped to the revision the
/// pass converges and the operator whose authority it runs under.
///
/// An implementation must answer only with provenance it can vouch is the
/// last successful projection of this client's current identity instance:
/// stale, missing, unavailable or unenrolled provenance is absent, never a
/// guess.
pub trait DeclarationLedgers: Send + Sync {
    /// The ledger one pass over `client` at `revision` reads and records in.
    fn for_pass(
        &self,
        client: &ClientId,
        revision: &ClientRevision,
        operator: &str,
    ) -> Arc<dyn DeclarationLedger>;
}

/// A reconciler acting as this operator, or the reason there is none.
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

    Ok(IdentityReconciler::new(factory.acting_as(operator.token())))
}

/// The ledgers a sweep scopes per client, with the operator it runs as.
pub(crate) struct PassLedgers<'a> {
    /// Where each client's ledger comes from.
    pub(crate) ledgers: &'a dyn DeclarationLedgers,
    /// The operator subject the records name.
    pub(crate) operator: &'a str,
}

impl PassLedgers<'_> {
    /// `reconciler`, keeping `client`'s ledger at `revision`.
    pub(crate) fn scope(
        &self,
        reconciler: &IdentityReconciler,
        client: &ClientId,
        revision: &ClientRevision,
    ) -> IdentityReconciler {
        reconciler
            .clone()
            .with_ledger(self.ledgers.for_pass(client, revision, self.operator))
    }
}

/// The ledgers `state` holds for a pass as `operator`, if any.
pub(crate) fn for_operator<'a>(
    state: &'a ControlPlaneState,
    operator: &'a Operator,
) -> Option<PassLedgers<'a>> {
    state.declaration_ledgers.as_deref().map(|ledgers| PassLedgers {
        ledgers,
        operator: operator.subject(),
    })
}
