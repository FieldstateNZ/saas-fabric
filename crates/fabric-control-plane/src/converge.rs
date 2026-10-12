//! Running a convergence pass with an operator's authority.
//!
//! Every path into the identity provider goes through here, and every one of
//! them starts with an operator. There is no other way in: the platform holds
//! no credential of its own, so a pass without somebody to authorise it cannot
//! be constructed (ADR 0012).
//!
//! In the 121–150 line band: the three ways in — a sweep, a sweep after a
//! write, and one client's re-assertion — share the one constructor that
//! binds a reconciler to an operator. Keeping them together is what makes
//! "every path starts with an operator" checkable in one file.

use fabric_client_model::{ClientId, ClientRevision};
use fabric_reconciliation::IdentityReconciler;

use crate::repository::StoredClient;
use crate::state::ControlPlaneState;
use crate::{audit, logging, reconcile, ControlPlaneError, Operator};

/// A reconciler acting as this operator, or the reason there is none.
fn reconciler(
    state: &ControlPlaneState,
    operator: &Operator,
) -> Result<IdentityReconciler, ControlPlaneError> {
    let factory = state
        .identity_provider
        .as_ref()
        .ok_or(ControlPlaneError::ConvergenceUnavailable)?;

    Ok(IdentityReconciler::new(factory.acting_as(operator.token())))
}

/// Converges every client, acting as this operator.
///
/// Returns how many clients were swept.
///
/// # Errors
///
/// Returns [`ControlPlaneError::ConvergenceUnavailable`] when this deployment
/// converges nothing. There is no "no authority" case: an operator always
/// carries one, because the only posture that could not supply one no longer
/// exists.
pub(crate) async fn sweep(
    state: &ControlPlaneState,
    operator: &Operator,
) -> Result<usize, ControlPlaneError> {
    let reconciler = reconciler(state, operator)?;

    let clients = reconcile::run(
        state.desired_state.current().as_ref(),
        &reconciler,
        state.service.statuses(),
        state.health.as_ref(),
        state.service.clock(),
    )
    .await;

    Ok(clients)
}

/// Re-asserts one client's identity projection, as this operator: converges
/// it and overwrites every unsupported hand edit the provider holds
/// (D01-19b), then records the pass and audits the request.
///
/// The operator names the revision they reviewed. A client whose desired
/// state has moved since is refused, so nobody overwrites a hand edit on the
/// strength of a declaration they did not see.
///
/// Not gated by the client's lifecycle: identity remediation stays open on a
/// retired client, as identity edits do. It uses the operator's own
/// authority, exactly as a sweep does, and needs no permission a sweep does
/// not.
///
/// # Errors
///
/// [`ControlPlaneError::ConvergenceUnavailable`] when this deployment
/// converges nothing, [`ControlPlaneError::RevisionConflict`] when `expected`
/// is stale, and whatever reading the client returns. A provider failure is
/// not an error here: it is recorded as the client's `failed` status, and
/// audited, like any pass.
pub(crate) async fn reassert(
    state: &ControlPlaneState,
    operator: &Operator,
    client: &ClientId,
    expected: &ClientRevision,
) -> Result<StoredClient, ControlPlaneError> {
    let reconciler = reconciler(state, operator)?;
    let stored = state.service.get(client).await?;

    if stored.revision != *expected {
        return Err(ControlPlaneError::RevisionConflict);
    }

    let outcome = reconciler.reassert(stored.document.client()).await;
    let status = state.service.statuses().record(
        client,
        stored.revision.clone(),
        &outcome,
        state.service.clock().now_unix_seconds(),
    );
    logging::client_reconciled(client, status, outcome.actions());

    state
        .service
        .audit()
        .record(audit::identity_projection_reasserted(
            operator,
            client,
            &stored.revision,
            &outcome,
            status,
        ))
        .await;

    Ok(stored)
}

/// Converges in the background, having already answered the operator.
///
/// Used after a write, where the response is committed before convergence is
/// attempted — the API's contract is that a write reports `pending` and
/// convergence is a separate event that can fail independently (ADR 0008).
///
/// Failures are logged rather than returned, because there is nobody left to
/// return them to; the client's recorded status carries the outcome, and that
/// is what the console reads.
pub(crate) fn in_background(state: &ControlPlaneState, operator: &Operator) {
    if state.identity_provider.is_none() {
        return;
    }

    let state = state.clone();
    let operator = operator.clone();

    tokio::spawn(async move {
        if let Err(error) = sweep(&state, &operator).await {
            tracing::warn!(
                event = "control_plane.convergence_skipped",
                detail = %error,
                "could not converge after a write"
            );
        }
    });
}
