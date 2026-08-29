//! Running a convergence pass with an operator's authority.
//!
//! Every path into the identity provider goes through here, and every one of
//! them starts with an operator. There is no other way in: the platform holds
//! no credential of its own, so a pass without somebody to authorise it cannot
//! be constructed (ADR 0012).

use fabric_reconciliation::IdentityReconciler;

use crate::state::ControlPlaneState;
use crate::{reconcile, ControlPlaneError, Operator};

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
    let factory = state
        .identity_provider
        .as_ref()
        .ok_or(ControlPlaneError::ConvergenceUnavailable)?;

    let reconciler = IdentityReconciler::new(factory.acting_as(operator.token()));

    Ok(reconcile::run(
        state.desired_state.current().as_ref(),
        &reconciler,
        state.service.statuses(),
        state.health.as_ref(),
        state.service.clock(),
    )
    .await)
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
