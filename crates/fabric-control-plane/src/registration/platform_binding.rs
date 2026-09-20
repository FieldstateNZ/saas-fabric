//! Platform Management, and the one environment a deployment manages.

use std::sync::Arc;

/// Platform Management, and the environment it manages.
///
/// # Why the environment is not a request parameter
///
/// It reaches the platform repository as a path segment —
/// `environments/<name>/components.yaml` — so a caller who could name it could
/// name a path. Specification section 31.7 forbids exactly that, and the
/// cheapest way to satisfy it is to have nowhere for a caller to say it: a
/// deployment manages one environment, stated in its configuration, and the
/// route takes no name at all.
#[derive(Clone)]
pub struct PlatformBinding {
    /// The service.
    pub service: Arc<fabric_platform_management::PlatformManagement>,

    /// The environment it manages, from this deployment's configuration.
    pub environment: String,

    /// The late-bound repository, so a connected integration can point it
    /// somewhere and a disconnected one can take it away.
    pub repository: Arc<fabric_platform_management::PlatformDesiredState>,

    /// Declaring and reading this environment's data sources (ADR 0023
    /// part 1).
    ///
    /// Built over the same late-bound `repository` above, so the two never
    /// disagree about which platform repository is live — see
    /// `fabric-control-plane-api`'s `startup::platform::establish`.
    pub data_sources: Arc<fabric_platform_management::DataSources>,

    /// Placing a client's data intent, and previewing what placing it would
    /// do (ADR 0023 part 2).
    ///
    /// Built over the same late-bound `repository` above and the same
    /// `data_sources` beside it, so a placement is decided against the
    /// same declared data sources an operator's own `GET
    /// /api/platform/data-sources` would show — see
    /// `fabric-control-plane-api`'s `startup::platform::establish`.
    pub placements: Arc<fabric_platform_management::Placements>,

    /// Publishes this environment's runtime documents, when this deployment
    /// has somewhere to publish them (ADR 0023 part 4).
    ///
    /// `None` where no publication target is configured — the route stays
    /// mounted and answers `PublicationNotConfigured` rather than 404, the
    /// same reasoning `platform`'s own absence gets one level up.
    /// `build_control_plane` is the only place this is ever `Some`: it is
    /// built from `ControlPlaneDeps.publication` once, there, over this
    /// binding's own `repository` above and the client desired-state
    /// binding — never rebuilt per request.
    pub publisher: Option<Arc<fabric_platform_management::RuntimePublisher>>,

    /// What the last publication pass found, and whether one is running.
    ///
    /// Always present, even when `publisher` is `None` — the alternative,
    /// `Option<Option<PublicationState>>`, would make "no publisher" and "a
    /// publisher that has not run yet" two states to tell apart for no
    /// reader's benefit. `GET /api/platform` decides whether to render a row
    /// at all from `publisher`, never from this being present, exactly as
    /// `ControlPlaneServices::platform_sweeps` is always built whether or
    /// not a platform is managed at all.
    pub publication: Arc<fabric_platform_management::PublicationState>,
}
