//! Wiring the control-plane domain.
//!
//! Past 120 lines for one reason: `build_control_plane` is the whole graph,
//! and splitting the one function that assembles it would hide half of it.

use std::sync::Arc;

use fabric_platform_management::{DataSourceState, DataSources, PublicationState};
use fabric_reconciliation::ReconciliationStatusStore;

mod contract;
mod platform_binding;
mod publication_sink;
mod publisher;

pub use contract::{ControlPlaneDeps, ControlPlaneServices};
pub use platform_binding::PlatformBinding;
pub use publication_sink::PublicationSink;

use crate::resolution::ResolutionService;
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
        audit,
        keys,
        identity_provider,
        git_integration,
        client_secrets: secret_store,
        operators,
        platform,
        platform_integration,
        publication,
        reserved_realms,
        reserved_client_ids,
        registries,
        resolution,
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

    // Built whether or not a publisher exists: see
    // `PlatformBinding::publication`'s own rustdoc.
    let publication_state = Arc::new(PublicationState::new());

    let service = Arc::new(ClientService::new(
        Arc::clone(repository),
        Arc::clone(&statuses),
        Arc::clone(&clock),
        audit,
        Arc::new(reserved_realms),
        Arc::new(reserved_client_ids),
    ));

    // The publisher is built here, not passed in whole: it needs the client
    // desired-state binding and the client service, which only this
    // function has in scope beside the platform binding.
    let platform = platform.map(|binding| {
        let publisher = publication
            .as_ref()
            .map(|sink| publisher::build(&binding, sink, repository, &service, &clock));

        // Declarations are placed above what was published (O3).
        let data_sources = publication.as_ref().map_or_else(
            || Arc::clone(&binding.data_sources),
            |sink| {
                let state = Arc::clone(&binding.repository) as Arc<dyn DataSourceState>;
                Arc::new(DataSources::new(state).with_publication(Arc::clone(&sink.target)))
            },
        );
        PlatformBinding {
            publisher,
            publication: Arc::clone(&publication_state),
            data_sources,
            ..binding
        }
    });
    let publisher = platform.as_ref().and_then(|binding| binding.publisher.clone());

    let resolution = Arc::new(ResolutionService::new(
        resolution,
        Arc::clone(&registries),
        Arc::clone(&clock),
    ));

    let client_secrets =
        secret_store.map(|store| Arc::new(crate::SecretsService::new(Arc::clone(&service), store)));

    logging::control_plane_ready(&described, &operators.describe());

    let router = control_plane_routes(ControlPlaneState {
        service,
        client_secrets,
        operators,
        git_integration,
        identity_provider,
        public_base_url: config.public_base_url.clone(),
        desired_state: Arc::clone(repository),
        health: Arc::clone(&health),
        platform,
        platform_integration,
        platform_sweeps: Arc::clone(&platform_sweeps),
        registries,
        resolution,
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
