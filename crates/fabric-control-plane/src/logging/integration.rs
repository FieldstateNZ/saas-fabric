//! Events about the platform's own Git integration.
//!
//! Split from the rest because they are about a different subject: everything
//! else records what happened to a *client*, and these record what happened to
//! the platform's connection to where clients are kept.
//!
//! Nothing here logs a correlation token, a private key, or an installation
//! token. Those are bearers, and a log is not a place for bearers.

use fabric_core::{event_id, EventType};

use crate::DOMAIN_ID;

/// An operator started a leg of the Git connection flow.
///
/// The operator is named because §24 requires every change to the platform's
/// own configuration to be attributable, and starting this flow is the first
/// half of one.
pub(crate) fn integration_flow_started(operator: &str, step: &str) {
    tracing::info!(
        event = "control_plane.integration_flow_started",
        event_id = event_id(DOMAIN_ID, EventType::Success, 5),
        operator,
        step,
        "operator started the Git connection flow"
    );
}

/// The platform's application was created on the Git host.
///
/// The slug is a public identifier. The private key that arrived with it is
/// not mentioned, not counted, and not described.
pub(crate) fn integration_app_created(operator: &str, app_slug: &str) {
    tracing::info!(
        event = "control_plane.integration_app_created",
        event_id = event_id(DOMAIN_ID, EventType::Success, 6),
        operator,
        app_slug,
        "created the platform's application on the Git host"
    );
}

/// An installation was recorded, having been proven to work.
pub(crate) fn integration_installed(operator: &str, installation: &str, settled: bool) {
    tracing::info!(
        event = "control_plane.integration_installed",
        event_id = event_id(DOMAIN_ID, EventType::Success, 7),
        operator,
        installation,
        repository_settled = settled,
        "recorded a mint-verified installation"
    );
}

/// An operator chose which repository holds client desired state.
pub(crate) fn integration_repository_chosen(operator: &str, owner: &str, name: &str) {
    tracing::info!(
        event = "control_plane.integration_repository_chosen",
        event_id = event_id(DOMAIN_ID, EventType::Success, 8),
        operator,
        repository = format!("{owner}/{name}"),
        "operator chose the client desired-state repository"
    );
}

/// Desired state is now being read through an established integration.
pub(crate) fn integration_bound(repository: &str) {
    tracing::info!(
        event = "control_plane.integration_bound",
        event_id = event_id(DOMAIN_ID, EventType::Success, 9),
        repository,
        "bound desired state to the connected repository"
    );
}

/// An operator disconnected the integration.
pub(crate) fn integration_disconnected(operator: &str) {
    tracing::warn!(
        event = "control_plane.integration_disconnected",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 3),
        operator,
        "operator disconnected the Git integration; desired state is unreachable until one is \
         connected again"
    );
}

/// A transition finished without anything watching it finish.
///
/// The task it runs in panicked, or the runtime is shutting down. The operator
/// is told the platform is unavailable, which is all anybody can honestly say:
/// this is not a transition that failed, it is one nothing saw the end of, and
/// the record and the live binding may or may not have both been written.
///
/// Errored rather than warned, because it is the one outcome that can leave
/// those two disagreeing, and nothing else will report it.
pub(crate) fn integration_transition_unobserved() {
    tracing::error!(
        event = "control_plane.integration_transition_unobserved",
        event_id = event_id(DOMAIN_ID, EventType::Error, 4),
        "an integration transition was not observed to finish; the stored record and the live \
         binding may not agree"
    );
}

/// A transition was turned away because the integration had moved under it.
///
/// Warned rather than errored, beside `operator_refused`: nothing failed and
/// nothing was written, but somebody's click did not take effect and the only
/// record of why is here. The operator is named nowhere in it — this is the one
/// integration event where no change landed, so there is nothing to attribute,
/// and the request that was turned away is already accounted for by the `409`
/// its caller received.
pub(crate) fn integration_transition_moved() {
    tracing::warn!(
        event = "control_plane.integration_transition_moved",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 5),
        "an integration transition was prepared against state that has since moved; nothing was \
         written"
    );
}

/// A stored integration could not be restored at startup.
///
/// Warned rather than errored, and the process continues. The platform reports
/// itself unconfigured, which is true, and the console still loads — which is
/// what somebody needs in order to do anything about this.
pub(crate) fn integration_restore_failed(detail: &str) {
    tracing::warn!(
        event = "control_plane.integration_restore_failed",
        event_id = event_id(DOMAIN_ID, EventType::Warning, 4),
        detail,
        "could not restore the stored Git integration; reporting as unconfigured"
    );
}
