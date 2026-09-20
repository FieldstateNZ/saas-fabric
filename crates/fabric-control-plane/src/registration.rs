//! Wiring the control-plane domain.

use std::sync::Arc;

use fabric_platform_management::{
    PlatformRepository, PublicationState, RuntimeCatalogueSource, RuntimePublisher,
};
use fabric_reconciliation::ReconciliationStatusStore;

mod contract;
mod platform_binding;
mod publication_sink;

pub use contract::{ControlPlaneDeps, ControlPlaneServices};
pub use platform_binding::PlatformBinding;
pub use publication_sink::PublicationSink;

use crate::routes::control_plane_routes;
use crate::service::{ClientService, DesiredStateCatalogueSource};
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
        platform_integration,
        publication,
        reserved_realms,
        reserved_client_ids,
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

    // Built once here, whether or not a publisher exists, so `GET
    // /api/platform` can tell "not configured" (`publisher: None`) apart
    // from "configured, nothing has run yet" (`publisher: Some`, no last
    // pass) without an `Option<Option<PublicationState>>` -- see
    // `PlatformBinding::publication`'s own rustdoc.
    let publication_state = Arc::new(PublicationState::new());

    // The publisher is constructed here, not passed in whole: it needs the
    // *client* desired-state binding (`repository`, above) to answer
    // `RuntimeCatalogueSource`, and only this function ever has both that
    // and the platform binding in scope at once.
    let platform = platform.map(|binding| {
        let publisher = publication.as_ref().map(|sink| {
            let source: Arc<dyn RuntimeCatalogueSource> =
                Arc::new(DesiredStateCatalogueSource::new(Arc::clone(repository)));

            Arc::new(RuntimePublisher::new(
                binding.environment.clone(),
                Arc::clone(&binding.repository) as Arc<dyn PlatformRepository>,
                source,
                Arc::clone(&sink.target),
                Arc::clone(&clock),
            ))
        });

        PlatformBinding {
            publisher,
            publication: Arc::clone(&publication_state),
            ..binding
        }
    });
    let publisher = platform.as_ref().and_then(|binding| binding.publisher.clone());

    let service = Arc::new(ClientService::new(
        Arc::clone(repository),
        Arc::clone(&statuses),
        clock,
        Arc::new(reserved_realms),
        Arc::new(reserved_client_ids),
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
        platform_integration,
        platform_sweeps: Arc::clone(&platform_sweeps),
    });

    Ok(ControlPlaneServices {
        router,
        statuses,
        health,
        platform_sweeps,
        publisher,
        publication: publication_state,
    })
}
