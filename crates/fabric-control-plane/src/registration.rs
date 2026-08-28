//! Wiring the control-plane domain.

use std::sync::Arc;

use axum::Router;
use fabric_core::Clock;
use fabric_reconciliation::ReconciliationStatusStore;

use crate::repository::ClientRepository;
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
    repository: Arc<dyn ClientRepository>,
    clock: Arc<dyn Clock>,
    keys: Arc<crate::KeyHolder>,
    sign_in: Option<Arc<crate::SignInSurface>>,
) -> Result<ControlPlaneServices, String> {
    let operators: Arc<dyn crate::OperatorAuthenticator> = Arc::from(config.operator.build(keys)?);
    let described = repository.describe();
    let statuses = Arc::new(ReconciliationStatusStore::new());
    let trigger = Arc::new(ReconciliationTrigger::new());

    let service = Arc::new(ClientService::new(
        repository,
        Arc::clone(&statuses),
        Arc::clone(&trigger),
        clock,
    ));

    logging::control_plane_ready(&described, &operators.describe());

    let router = control_plane_routes(ControlPlaneState {
        service,
        operators,
        sign_in,
    });

    Ok(ControlPlaneServices {
        router,
        statuses,
        trigger,
    })
}
