//! Wiring the control-plane domain.

use std::sync::Arc;

use axum::Router;
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;

use crate::repository::DesiredStateBinding;
use crate::routes::control_plane_routes;
use crate::service::ClientService;
use crate::state::ControlPlaneState;
use crate::{logging, ControlPlaneConfig};

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

    /// Establishes who an operator is, when something other than the
    /// configured posture should decide.
    ///
    /// `None` in every deployment: the posture in configuration is what builds
    /// it. It exists for tests, which drive the real router and would
    /// otherwise have to mint tokens signed by a key they also had to publish
    /// — proving the extractor works, and nothing else, at considerable cost.
    pub operators: Option<Arc<dyn crate::OperatorAuthenticator>>,
}

/// Validates configuration, builds the service, and returns its router.
///
/// # Errors
///
/// Returns a message if the operator posture cannot be built — a blank issuer,
/// client or role. Each means the API would be reachable by nobody or by
/// everybody, and finding that out at startup beats finding it out when an
/// operator cannot sign in.
pub fn build_control_plane(
    config: &ControlPlaneConfig,
    deps: ControlPlaneDeps,
) -> Result<ControlPlaneServices, String> {
    let ControlPlaneDeps {
        desired_state: repository,
        clock,
        keys,
        identity_provider,
        sign_in,
        git_integration,
        operators,
    } = deps;

    let repository = &repository;
    let operators: Arc<dyn crate::OperatorAuthenticator> = match operators {
        Some(supplied) => supplied,
        None => Arc::from(config.operator.build(keys)?),
    };
    let described = repository.current().describe();
    let statuses = Arc::new(ReconciliationStatusStore::new());
    let health = Arc::new(crate::IntegrationHealth::new());

    let service = Arc::new(ClientService::new(
        Arc::clone(repository),
        Arc::clone(&statuses),
        clock,
    ));

    logging::control_plane_ready(&described, &operators.describe());

    let router = control_plane_routes(ControlPlaneState {
        service,
        operators,
        sign_in,
        git_integration,
        identity_provider,
        public_base_url: config.public_base_url.clone(),
        desired_state: Arc::clone(repository),
        health: Arc::clone(&health),
    });

    Ok(ControlPlaneServices {
        router,
        statuses,
        health,
    })
}
