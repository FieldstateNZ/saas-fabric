//! Wiring the control-plane domain.

use std::sync::Arc;

use fabric_reconciliation::ReconciliationStatusStore;

mod contract;

pub use contract::{ControlPlaneDeps, ControlPlaneServices};

use crate::routes::control_plane_routes;
use crate::service::ClientService;
use crate::state::ControlPlaneState;
use crate::{logging, ControlPlaneConfig};

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
        client_secrets: secret_store,
        operators,
        platform,
    } = deps;

    let repository = &repository;
    let operators: Arc<dyn crate::OperatorAuthenticator> = match operators {
        Some(supplied) => supplied,
        None => Arc::from(config.operator.build(keys)?),
    };
    let described = repository.current().describe();
    let statuses = Arc::new(ReconciliationStatusStore::new());
    let platform_sweeps = Arc::new(fabric_platform_management::SweepState::default());
    let health = Arc::new(crate::IntegrationHealth::new());

    let service = Arc::new(ClientService::new(
        Arc::clone(repository),
        Arc::clone(&statuses),
        clock,
    ));

    let client_secrets =
        secret_store.map(|store| Arc::new(crate::SecretsService::new(Arc::clone(&service), store)));

    logging::control_plane_ready(&described, &operators.describe());

    let router = control_plane_routes(ControlPlaneState {
        service,
        client_secrets,
        operators,
        sign_in,
        git_integration,
        identity_provider,
        public_base_url: config.public_base_url.clone(),
        desired_state: Arc::clone(repository),
        health: Arc::clone(&health),
        platform,
        platform_sweeps: Arc::clone(&platform_sweeps),
    });

    Ok(ControlPlaneServices {
        router,
        statuses,
        health,
        platform_sweeps,
    })
}
