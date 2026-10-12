//! `POST /api/clients/{clientId}/identity/reassertion`

use axum::extract::State;
use http::HeaderMap;

use crate::extraction::ClientPath;
use crate::models::IdentityResponse;
use crate::state::ControlPlaneState;
use crate::{preconditions, ControlPlaneError, Operator};

/// Re-asserts a client's identity projection: writes its declared identity
/// over every unsupported hand edit the identity provider holds (D01-19b).
///
/// # The explicit, audited way back from `handEdited`
///
/// An ordinary pass leaves a hand edit it does not support in place and
/// reports it, rather than overwriting it on every sweep. An operator then
/// either removes the edit in the provider, or asks for this. It runs with
/// the operator's own authority, as every convergence does (ADR 0012), and
/// is recorded in the audit trail whatever it concludes.
///
/// `If-Match` names the revision the operator reviewed; a client whose
/// desired state has moved since is refused with `409 revision_conflict`.
/// The response is the client's identity with the pass's outcome as its
/// reconciliation state — synchronous, unlike an identity edit, because
/// this request *is* the convergence rather than a write that precedes one.
pub(crate) async fn reassert_identity(
    State(state): State<ControlPlaneState>,
    ClientPath(client_id): ClientPath,
    operator: Operator,
    headers: HeaderMap,
) -> Result<IdentityResponse, ControlPlaneError> {
    let expected = preconditions::required_revision(&headers)?;

    let stored = crate::converge::reassert(&state, &operator, &client_id, &expected).await?;
    let reconciliation = state.service.reconciliation(&stored);

    Ok(IdentityResponse::new(&stored, reconciliation))
}
