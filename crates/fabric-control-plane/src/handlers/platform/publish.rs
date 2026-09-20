//! `POST /api/platform/publication`: publish the runtime's three documents
//! now.

use axum::extract::State;
use axum::Json;
use fabric_core::{Clock, SystemClock};
use fabric_platform_management::PassResult;

use crate::handlers::platform::body::{pass_outcome_word, PublicationRow};
use crate::state::ControlPlaneState;
use crate::{audit, ControlPlaneError, Operator};

/// `POST /api/platform/publication`.
///
/// # It is a POST because it is an act
///
/// The same reasoning as [`super::roll_back_component`]: an operator is not
/// replacing a resource they composed, they are asking the platform to run a
/// pass now rather than wait for the schedule. What gets composed and
/// offered is the platform's to decide, same as every other pass.
///
/// # Why this exists at all
///
/// The schedule ([`fabric_platform_management::RuntimePublisher::publish_once`],
/// started by the host on an interval) is what keeps the runtime current in
/// steady state. This is "look now" for the same reason `POST
/// /api/reconciliation` is: an operator who just declared a data source or
/// placed a tenant should not have to wait out the interval to see it take
/// effect, or to find out it was refused.
///
/// # Errors
///
/// [`ControlPlaneError::PlatformNotManaged`] if this deployment manages no
/// platform at all; [`ControlPlaneError::PublicationNotConfigured`] if it
/// manages one but publishes no runtime state; [`ControlPlaneError::PublicationRunning`]
/// if another pass -- scheduled or triggered -- is already in flight.
pub(crate) async fn publish_runtime_state(
    operator: Operator,
    State(state): State<ControlPlaneState>,
) -> Result<Json<PublicationRow>, ControlPlaneError> {
    let platform = state.platform()?;
    let publisher = platform
        .publisher
        .as_ref()
        .ok_or(ControlPlaneError::PublicationNotConfigured)?;

    match publisher.publish_once(&platform.publication).await {
        PassResult::AlreadyRunning => Err(ControlPlaneError::PublicationRunning),
        PassResult::Ran(outcome) => {
            audit::publication_triggered(&operator, pass_outcome_word(&outcome));

            // Rendered from the outcome this call just produced, not a
            // second `PublicationState` read: the guard is already
            // released by the time `publish_once` returns (`RunningGuard`
            // drops as its own call frame unwinds), so a scheduled pass
            // could start and finish in the gap before a fresh read, and
            // this response would then describe a pass the operator did
            // not ask for.
            let at_unix_seconds = SystemClock::new().now_unix_seconds();
            Ok(Json(PublicationRow::at(publisher, at_unix_seconds, &outcome)))
        }
    }
}
