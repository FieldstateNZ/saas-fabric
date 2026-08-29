//! Reporting whether the platform can reach client desired state.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::integration::IntegrationStatus;
use crate::state::ControlPlaneState;
use crate::Operator;

/// What an operator is told about the desired-state integration.
///
/// # Status, never credentials
///
/// Nothing here is secret and nothing here is a reference to something secret.
/// No token, no key, no key *name*, and no path: an operator is told whether
/// the platform can read desired state, where from in the terms they connected
/// it, and when it last worked. Section 15 makes that a rule rather than a
/// habit, and `scripts/check_architecture.py` checks the console never learns
/// otherwise.
#[derive(Serialize)]
pub(crate) struct IntegrationReport {
    /// What state the connection is in.
    status: IntegrationStatus,

    /// How the connected repository describes itself, in operator terms.
    ///
    /// `None` when nothing is connected. This is a sentence for a human, not
    /// a structured location — the structured form arrives with the flow that
    /// establishes it, and inventing one here would mean guessing its shape.
    connection: Option<String>,

    /// When desired state was last read successfully, in Unix seconds.
    ///
    /// Usually the first question asked about a broken integration, which is
    /// why it survives the failure that broke it.
    last_success_at: Option<u64>,
}

/// Reports the desired-state integration.
///
/// Takes an [`Operator`] like every other client-facing handler. Integration
/// status is not public information: it says whether this platform is
/// connected to a repository and names it, which is exactly the reconnaissance
/// an unauthenticated caller should not get for free.
pub(crate) async fn get_integration(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Json<IntegrationReport> {
    let configured = state.desired_state.is_configured();

    Json(IntegrationReport {
        status: state.health.status(configured),
        connection: configured.then(|| state.desired_state.current().describe()),
        last_success_at: state.health.last_success(),
    })
}
