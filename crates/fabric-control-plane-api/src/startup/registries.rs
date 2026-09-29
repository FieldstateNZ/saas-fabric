//! Composing the image registries: the router Platform Management reads
//! through, and the service operators register registries with (ADR 0026
//! section 5).
//!
//! # One router
//!
//! Platform Management reads every repository through [`Registries`], by the
//! repository's host; the registry service installs into the same router, so
//! a credential an operator sets is the one discovery presents from the
//! moment the change is reported done, and a host nobody registered is
//! refused by name rather than read from a default.
//!
//! # The deployment's registry is configuration, and keeps its own rules
//!
//! When Platform Management is configured, `[platform_management.registry]`
//! places the deployment's own registry — on any network the deployment
//! chooses, following its own challenge — and it is read anonymously until
//! an operator registers its host at its endpoint with a credential.
//! Registries an operator registers are read at public addresses only.

mod client;
mod composition;
mod connector;
#[cfg(test)]
mod connector_tests;
mod deployment;
mod kind;

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_control_plane::{DeploymentRegistry, RegistryConnector, RegistryService, RegistryServiceParts};
use fabric_core::Clock;
use fabric_registry::Registries;

use crate::config::ControlPlaneAppConfig;
use crate::startup::integration::InstanceStores;

pub use composition::{ComposedRegistries, RegistryComposition};

/// Builds the router and the service over it, and restores what was
/// registered before the last restart.
///
/// # Errors
///
/// A message naming the field if the deployment's registry cannot be built
/// or a timeout is zero: configuration stated wrongly is fatal. Nothing an
/// operator registered is: restoring logs what it could not rebuild.
pub async fn compose_registries(parts: RegistryComposition) -> Result<ComposedRegistries, String> {
    if parts.http_timeout_seconds == 0 {
        return Err("registries.http_timeout_seconds is zero".to_owned());
    }
    let deployment = parts
        .deployment
        .as_ref()
        .map(deployment::Deployment::build)
        .transpose()?;

    let mut by_host = BTreeMap::new();
    if let Some(deployment) = &deployment {
        by_host.insert(deployment.host.clone(), Arc::clone(&deployment.anonymous));
    }
    let router = Arc::new(Registries::new(by_host));

    let described = deployment
        .as_ref()
        .map(|deployment| DeploymentRegistry::new(&deployment.host, &deployment.base_url));
    let connector = connector::Connector::new(Arc::clone(&router), deployment, parts.http_timeout_seconds);
    let service = Arc::new(RegistryService::new(RegistryServiceParts {
        store: parts.store,
        secrets: parts.secrets,
        connector: Arc::new(connector) as Arc<dyn RegistryConnector>,
        clock: parts.clock,
        deployment: described,
    }));
    // Never fatal, and never given up on: a store that did not answer now is
    // asked again in the background until it does.
    if !service.restore().await {
        tokio::spawn(Arc::clone(&service).restore_until_complete());
    }

    Ok(ComposedRegistries { service, router })
}

/// Composes the registries configuration describes, over this instance's stores.
///
/// # Errors
///
/// As [`compose_registries`].
pub(super) async fn establish(
    config: &ControlPlaneAppConfig,
    stores: &InstanceStores,
    clock: &Arc<dyn Clock>,
) -> Result<ComposedRegistries, String> {
    compose_registries(RegistryComposition {
        deployment: config
            .platform_management
            .as_ref()
            .map(|managed| managed.registry.clone()),
        http_timeout_seconds: config.registries.http_timeout_seconds,
        store: Arc::clone(&stores.registries),
        secrets: Arc::clone(&stores.secrets),
        clock: Arc::clone(clock),
    })
    .await
}
