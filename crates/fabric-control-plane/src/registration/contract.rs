//! What the host hands in, and what it gets back.

use std::sync::Arc;

use axum::Router;
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;

use crate::repository::DesiredStateBinding;

/// What building the control plane produces.
///
/// The router is the obvious half. The other three are handed back because the
/// **reconciliation loop is the host's to start**, not this function's: a
/// process that only wants to serve the API — a test, or a replica deliberately
/// running read-only — should be able to have one without a background task
/// sweeping every client behind it.
pub struct ControlPlaneServices {
    /// The HTTP surface.
    pub router: Router,

    /// What is known about whether desired state has taken effect.
    pub statuses: Arc<ReconciliationStatusStore>,

    /// What the last sweep observed about reading desired state.
    ///
    /// Held by the loop, which records into it, and by the API, which reports
    /// it. Nothing else needs it, and nothing else may write to it.
    pub health: Arc<crate::IntegrationHealth>,

    /// What the last platform sweep found, and whether one is running.
    ///
    /// Handed back for the same reason as the reconciliation loop: **starting
    /// the sweep is the host's**, on the cadence its deployment configures. A
    /// process that only wants to serve the API should be able to, without a
    /// background task advancing environments behind it.
    pub platform_sweeps: Arc<fabric_platform_management::SweepState>,
}

/// What the control plane is assembled from.
///
/// A struct rather than eight positional parameters. Half of them are
/// `Option<Arc<dyn …>>` and three of those are interchangeable at the call
/// site by type, which is the shape of argument list where a transposition
/// compiles and then behaves strangely at runtime.
pub struct ControlPlaneDeps {
    /// Where desired state is read and written, or the fact that it is not.
    pub desired_state: Arc<DesiredStateBinding>,

    /// Stamps writes and reconciliation outcomes.
    pub clock: Arc<dyn Clock>,

    /// The keys operator tokens are verified against.
    pub keys: Arc<crate::KeyHolder>,

    /// Lends each operator's authority to the identity provider.
    pub identity_provider: Option<Arc<dyn crate::IdentityProviderFactory>>,

    /// How an operator obtains a token.
    pub sign_in: Option<Arc<crate::SignInSurface>>,

    /// The Git connection flow, when this deployment manages its own.
    pub git_integration: Option<Arc<crate::GitIntegrationService>>,

    /// Where clients' secrets live, when a deployment has a store for them.
    ///
    /// `None` leaves the secrets routes mounted and answering "this client has
    /// no secret boundary" — the same answer a client without one gets, and
    /// for the same reason: the console can tell an operator what is missing
    /// rather than meeting a route that does not exist.
    pub client_secrets: Option<Arc<dyn crate::ClientSecrets>>,

    /// The flow that connects the platform repository, when this deployment
    /// manages an environment.
    ///
    /// Separate from `platform` because they become available at different
    /// times: the binding exists from startup and holds nothing, and this is
    /// what an operator uses to give it something to hold.
    pub platform_integration: Option<Arc<crate::GitIntegrationService>>,

    /// Platform Management, and the one environment it manages.
    ///
    /// One struct rather than two optional fields, because two that must agree
    /// is a shape that can disagree. A control plane manages the environment
    /// it was deployed into; there is no second one to name.
    ///
    /// `None` leaves the route mounted and answering that nothing is managed,
    /// so a console can say what is missing rather than meeting a 404 it would
    /// have to guess the meaning of.
    pub platform: Option<PlatformBinding>,

    /// Establishes who an operator is, when something other than the
    /// configured posture should decide.
    ///
    /// `None` in every deployment: the posture in configuration is what builds
    /// it. It exists for tests, which drive the real router and would
    /// otherwise have to mint tokens signed by a key they also had to publish
    /// — proving the extractor works, and nothing else, at considerable cost.
    pub operators: Option<Arc<dyn crate::OperatorAuthenticator>>,
}

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
}
