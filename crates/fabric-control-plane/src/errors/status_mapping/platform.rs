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
///
/// One pair here does share a code too: `InvalidDataSource`'s structural
/// arm answers the same 422 `NotRollable` does, for unrelated reasons.
/// Saying so is better than merging them into an arm with two causes.
#[allow(
    clippy::match_same_arms,
    reason = "arms are grouped by cause; the comment above says which ones share a status"
)]
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

        // A declared data source breaks one of ADR 0023 part 1's rules.
        // Structural, so this is answered the same way whether a handler
        // unwraps `PlatformError::InvalidDataSource` itself or lets `?` wrap
        // it in `ControlPlaneError::Platform` -- 422, joining `NotRollable`:
        // understood, and what was asked for cannot be acted on, so the fix
        // is to change what was submitted rather than to retry it.
        PlatformError::InvalidDataSource(_) => StatusCode::UNPROCESSABLE_ENTITY,

        // A held data-sources document a hand edit made incoherent -- a
        // duplicate id, or an entry that no longer validates
        // (`data_sources::held::check_held`, run on every read). Not the
        // caller's fault and no retry fixes it, so it shares
        // `ControlPlaneError::InvalidDesiredState`/`InvalidCatalogue`'s 500
        // rather than falling to the catch-all below -- unlike an
        // adapter's own `Refused` (a revoked credential, a host rejecting
        // a write, an unreadable or wrong-schema file), which is exactly
        // that catch-all's 503, the same as hold and rollback answer.
        PlatformError::InvalidHeldDataSources { .. } => StatusCode::INTERNAL_SERVER_ERROR,

        // The selector refused to place this intent (ADR 0023 part 2): no
        // declared data source admits the intent's class, region or
        // acceptance of new tenants, the tenant is already placed, or a
        // discriminator value collided. 422, joining `InvalidDataSource`
        // and `NotRollable`: understood, and what was asked for cannot be
        // acted on, so the fix is to change what was submitted -- declare
        // a data source that admits it -- rather than to retry it.
        PlatformError::PlacementRefused(_) => StatusCode::UNPROCESSABLE_ENTITY,

        // A held placements document a hand edit made incoherent --
        // `placements::held::check_held_placements`'s own refusal, the same
        // shape as `InvalidHeldDataSources` above and sharing its 500 for
        // the same reason: a coherence problem in a document this platform
        // itself authored, not an outage upstream of it.
        PlatformError::InvalidHeldPlacements { .. } => StatusCode::INTERNAL_SERVER_ERROR,

        // A data source cannot be removed while a placement still names it
        // (ADR 0023 part 2). 409, not 422: the request is well-formed and
        // would succeed once every placement it names is gone, so this is
        // a conflict with other state rather than something wrong with the
        // request itself -- the same reason a stale revision is 409 and
        // not 400.
        PlatformError::DataSourceInUse { .. } => StatusCode::CONFLICT,

        // Platform Management reached a registry or the platform repository and
        // could not get an answer. 503, not 500: nothing is wrong with the
        // request, desired state is untouched, and the operator's next step is
        // to look again shortly.
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}
