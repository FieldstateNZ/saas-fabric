//! Which status a Platform Management failure carries.

use fabric_platform_management::{DesiredStateError, PlatformError};
use http::StatusCode;

/// The status code an operator's browser sees for a platform failure.
///
/// # Why these are grouped by cause rather than by code
///
/// Two of these are 404 and two are 409, and merging either pair would put
/// unrelated failures in one arm and delete the comment explaining each. The
/// distinction an operator acts on survives in the machine code beside it,
/// where every one of them has its own — see `codes.rs`.
pub(super) fn status(error: &PlatformError) -> StatusCode {
    match error {
        // Nothing is connected, or the manifest does not name this component.
        // 404 beside the other "this deployment does not have one" answers,
        // because that is what both are: an absence. Neither is fixed by asking
        // again, so neither is a 503.
        //
        // Deliberately distinct from the catch-all below. A *connected*
        // repository that cannot be read is broken and needs looking at;
        // reporting that as "not connected" would send an operator to connect
        // something they already have.
        PlatformError::DesiredState(DesiredStateError::NotConnected | DesiredStateError::NotFound { .. }) => {
            StatusCode::NOT_FOUND
        }

        // 409, not 503 and not 400. The request is well-formed and was
        // understood; the component's state is what does not permit it, and an
        // operator's next step is to look at the policy rather than to retry or
        // to correct their request.
        //
        // A stale decision joins it, for the same reason. The state it was
        // decided against has moved — somebody added a hold, or an operator
        // rebound the platform to another repository — so it has to be taken
        // again against what is there now. Falling to the catch-all made this a
        // `503` with a `Retry-After` and a server-error log line, which is
        // wrong three times over: nothing is unavailable, an immediate retry
        // would be refused identically, and the operator's own click was being
        // recorded as a platform fault.
        PlatformError::NotAdvancing { .. } | PlatformError::DesiredState(DesiredStateError::Conflict) => {
            StatusCode::CONFLICT
        }

        // The version is not one this component can go back to. 422: the
        // request is well-formed and its content is what cannot be acted on —
        // and unlike a 404 there *is* a component here, it just has no such
        // release to return to.
        PlatformError::NotRollable { .. } => StatusCode::UNPROCESSABLE_ENTITY,


        // Platform Management reached a registry or the platform repository and
        // could not get an answer. 503, not 500: nothing is wrong with the
        // request, desired state is untouched, and the operator's next step is
        // to look again shortly.
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}
