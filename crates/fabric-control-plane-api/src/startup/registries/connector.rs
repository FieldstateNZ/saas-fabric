//! Building a registry client from what the control plane recorded, and
//! installing the set.
//!
//! Kept in one file because building a client and installing the set are the
//! port's two halves, and the deployment's host is held to one rule in both.
//!
//! # Every rule is the adapter's, chosen by kind
//!
//! This decides which constructor a registry is built with — its kind's —
//! and, for the deployment's own host, that it is served where the
//! deployment reads it, on the deployment's network. Nothing else: realms,
//! public addresses, redirects and where a credential may go are
//! `fabric-registry`'s, fixed by that constructor.
//!
//! # The deployment's host keeps its kind's realm
//!
//! An operator's `ghcr` credential for the deployment's `ghcr.io` goes only
//! to `https://ghcr.io/token`, and a `distribution` one only to the realm it
//! recorded, exactly as at any other host: the deployment contributes where
//! the registry is served and on which network, never where a credential
//! may go. A connection for that host at any endpoint but the deployment's
//! is refused, so a stored credential never moves to another endpoint.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_control_plane::{RegistryClient, RegistryConnection, RegistryConnector, RegistryHost};
use fabric_registry::{Credential, OciRegistry, Registries, RegistrySecret, RegistrySettings};

use super::client::Adapted;
use super::deployment::Deployment;
use super::kind::{by_kind, same};

/// Builds clients, and installs the set into the router Platform Management
/// reads through.
pub(super) struct Connector {
    /// The router.
    router: Arc<Registries>,

    /// The deployment's own registry, when there is one.
    deployment: Option<Deployment>,

    /// `[registries].http_timeout_seconds`.
    timeout_seconds: u64,
}

impl Connector {
    /// A connector installing into `router`.
    pub(super) fn new(router: Arc<Registries>, deployment: Option<Deployment>, timeout_seconds: u64) -> Self {
        Self {
            router,
            deployment,
            timeout_seconds,
        }
    }

    /// The settings a connection is built from, and its timeout.
    ///
    /// # Errors
    ///
    /// A message if the connection is for the deployment's host at another
    /// endpoint, or its kind's constructor refuses it.
    pub(super) fn settings(
        &self,
        connection: &RegistryConnection,
    ) -> Result<(RegistrySettings, u64), String> {
        let settings = by_kind(connection)?;
        match self
            .deployment
            .as_ref()
            .filter(|deployment| deployment.host == connection.host.as_str())
        {
            None => Ok((settings, self.timeout_seconds)),
            Some(deployment) if same(&deployment.base_url, &connection.endpoint) => Ok((
                settings.at_deployment(&deployment.base_url)?,
                deployment.timeout_seconds,
            )),
            Some(_) => Err(
                "registry: a registry for the deployment's host is read only at the deployment's base_url"
                    .to_owned(),
            ),
        }
    }
}

impl RegistryConnector for Connector {
    fn connect(&self, connection: RegistryConnection) -> Result<Arc<dyn RegistryClient>, String> {
        let (settings, timeout_seconds) = self.settings(&connection)?;
        let settings = match &connection.credential {
            None => settings,
            Some(credential) => settings.with_credential(
                Credential::new(
                    credential.username(),
                    RegistrySecret::new(credential.token().expose()),
                    connection.repositories.iter().map(ToString::to_string),
                )?
                .sharing_refusal(credential.refusal_mark()),
            ),
        };
        let registry = OciRegistry::with_settings(settings, timeout_seconds)?;
        Ok(Arc::new(Adapted(Arc::new(registry))))
    }

    fn install(&self, clients: BTreeMap<RegistryHost, Arc<dyn RegistryClient>>) {
        let mut by_host = BTreeMap::new();
        if let Some(deployment) = &self.deployment {
            by_host.insert(deployment.host.clone(), Arc::clone(&deployment.anonymous));
        }
        for (host, client) in clients {
            let client: Arc<dyn Any + Send + Sync> = client;
            // Only this connector's clients are ever handed back to it; one
            // that is not is not installed rather than guessed at.
            if let Ok(adapted) = client.downcast::<Adapted>() {
                by_host.insert(host.to_string(), Arc::clone(&adapted.0));
            } else {
                tracing::error!(
                    event = "control_plane.registry_not_installed",
                    host = host.as_str(),
                    "a registry client this connector did not build was not installed"
                );
            }
        }
        self.router.replace(by_host);
    }
}
