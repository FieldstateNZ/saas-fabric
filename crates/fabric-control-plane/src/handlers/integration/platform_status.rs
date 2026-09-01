//! Reporting the application that reaches the platform repository.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::application::{describe, Application};
use crate::state::ControlPlaneState;
use crate::Operator;

/// What an operator is told about the Platform Management integration.
///
/// # This is the application's lifecycle, not the environment's state
///
/// Whether the platform repository can actually be read, when it last was, and
/// what each component is doing are `GET /api/platform`'s to answer, and it
/// answers them from the binding this integration connects. Repeating any of it
/// here would be two routes reporting one fact, which is one route away from
/// them disagreeing.
///
/// What is only knowable here is the part before that: has an application been
/// created, has it been installed, has a repository been chosen.
#[derive(Serialize)]
pub(crate) struct PlatformIntegrationReport {
    /// Whether this deployment does platform management at all.
    ///
    /// `false` where no environment is configured. The console uses it to
    /// decide whether to offer the connection at all, rather than offering one
    /// that would connect a repository nothing would then read.
    managed: bool,

    /// The application this platform created, once it has one.
    application: Option<Application>,
}

/// Reports the Platform Management integration.
///
/// Takes an [`Operator`] for the same reason its client-side counterpart does:
/// this says whether the platform is connected to a repository and names it,
/// which is exactly the reconnaissance an unauthenticated caller should not
/// get for free.
pub(crate) async fn get_platform_integration(
    _operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Json<PlatformIntegrationReport> {
    // A store this platform cannot read is reported as no application rather
    // than as a failure of the whole report: the console's only view of the
    // problem should not be the thing the problem takes away.
    let application = match state.platform_integration.as_ref() {
        Some(service) => service.current().await.ok().flatten().as_ref().map(describe),
        None => None,
    };

    Json(PlatformIntegrationReport {
        managed: state.platform_integration.is_some(),
        application,
    })
}
