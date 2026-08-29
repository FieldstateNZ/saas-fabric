//! Wiring the control-plane domain.

use std::sync::Arc;

use axum::Router;
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;

use crate::repository::DesiredStateBinding;
use crate::routes::control_plane_routes;
use crate::service::ClientService;
use crate::state::ControlPlaneState;
use crate::{logging, ControlPlaneConfig, ReconciliationTrigger};

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

    /// Asks the reconciliation loop for a pass now.
    pub trigger: Arc<ReconciliationTrigger>,

    /// What the last sweep observed about reading desired state.
    ///
    /// Held by the loop, which records into it, and by the API, which reports
    /// it. Nothing else needs it, and nothing else may write to it.
    pub health: Arc<crate::IntegrationHealth>,
}

/// Validates configuration, builds the service, and returns its router.
///
/// # Errors
///
/// Returns a message if the operator posture cannot be built — an invalid
/// header name, or an empty allowlist. Both mean the API would be reachable by
/// nobody or by everybody, and finding that out at startup beats finding it out
/// when an operator cannot sign in.
pub fn build_control_plane(
    config: &ControlPlaneConfig,
    repository: &Arc<DesiredStateBinding>,
    clock: Arc<dyn Clock>,
    keys: Arc<crate::KeyHolder>,
    sign_in: Option<Arc<crate::SignInSurface>>,
) -> Result<ControlPlaneServices, String> {
    let operators: Arc<dyn crate::OperatorAuthenticator> = Arc::from(config.operator.build(keys)?);
    let described = repository.current().describe();
    let statuses = Arc::new(ReconciliationStatusStore::new());
    let health = Arc::new(crate::IntegrationHealth::new());
    let trigger = Arc::new(ReconciliationTrigger::new());

    let service = Arc::new(ClientService::new(
        Arc::clone(repository),
        Arc::clone(&statuses),
        Arc::clone(&trigger),
        clock,
    ));

    logging::control_plane_ready(&described, &operators.describe());

    let router = control_plane_routes(ControlPlaneState {
        service,
        operators,
        sign_in,
        desired_state: Arc::clone(repository),
        health: Arc::clone(&health),
    });

    Ok(ControlPlaneServices {
        router,
        statuses,
        trigger,
        health,
    })
}
