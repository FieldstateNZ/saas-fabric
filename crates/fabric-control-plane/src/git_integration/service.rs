//! Connecting this platform to where client desired state lives.
//!
//! # The order matters, and each step is only recorded once it is proven
//!
//! 1. An operator names the organisation. The platform describes the
//!    application it wants and hands the browser to the host.
//! 2. The host returns a one-time code. The platform redeems it, **stores the
//!    private key first**, and only then records the application — a record
//!    without its key is an integration that can never mint anything, and the
//!    key arrives exactly once.
//! 3. The operator installs the application. The platform **mints a token
//!    before recording the installation**, so a recorded installation always
//!    means a working one.
//! 4. If the installation reaches exactly one repository, that is the one. If
//!    it reaches several, the platform declines to guess and the operator
//!    chooses.

mod binding;
mod candidates;
mod connect;
mod disconnect;
mod errors;
mod install;

use std::sync::Arc;

use fabric_core::Clock;

use crate::git_integration::{
    DesiredStateFactory, GitAppProvisioning, IntegrationKind, IntegrationStore, PendingFlows, SecretStore,
};
use crate::repository::DesiredStateBinding;

pub use errors::IntegrationError;

/// Everything the connection flow needs.
pub struct GitIntegrationService {
    /// Which integration this instance is for.
    ///
    /// One service per integration, and it carries its own kind rather than
    /// taking one per call. A caller that could pass a kind could pass the
    /// wrong one, and the wrong one here means connecting client
    /// configuration and disconnecting platform management.
    kind: IntegrationKind,

    /// Creates and inspects the application on the host.
    provisioning: Arc<dyn GitAppProvisioning>,

    /// Where the private key goes.
    secrets: Arc<dyn SecretStore>,

    /// Where the record goes.
    store: Arc<dyn IntegrationStore>,

    /// Flows started and not yet completed.
    flows: Arc<PendingFlows>,

    /// Builds a repository once there is an integration to build one from.
    factory: Arc<dyn DesiredStateFactory>,

    /// What the rest of the control plane reads desired state through.
    binding: Arc<DesiredStateBinding>,

    /// Stamps flow expiry.
    clock: Arc<dyn Clock>,
}

impl GitIntegrationService {
    /// Assembles the service.
    #[must_use]
    pub fn new(
        kind: IntegrationKind,
        provisioning: Arc<dyn GitAppProvisioning>,
        secrets: Arc<dyn SecretStore>,
        store: Arc<dyn IntegrationStore>,
        factory: Arc<dyn DesiredStateFactory>,
        binding: Arc<DesiredStateBinding>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            kind,
            provisioning,
            secrets,
            store,
            flows: Arc::new(PendingFlows::new()),
            factory,
            binding,
            clock,
        }
    }

    /// The stored integration, if this platform has one.
    ///
    /// # Errors
    ///
    /// Returns [`IntegrationError`] if the store could not be read.
    pub async fn current(&self) -> Result<Option<crate::GitIntegration>, IntegrationError> {
        self.store.load(self.kind).await.map_err(IntegrationError::from)
    }
}
